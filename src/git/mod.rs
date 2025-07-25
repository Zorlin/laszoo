use std::path::{Path, PathBuf};
use git2::{Repository, Signature, IndexAddOption, Oid, StatusOptions, Status};
use serde::{Deserialize, Serialize};
use tracing::{info, debug, warn, error};
use crate::error::{LaszooError, Result};

pub struct GitManager {
    repo_path: PathBuf,
}

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

impl GitManager {
    pub fn new(repo_path: PathBuf) -> Self {
        Self { repo_path }
    }
    
    /// Initialize a git repository if it doesn't exist
    pub fn init_repo(&self) -> Result<Repository> {
        match Repository::open(&self.repo_path) {
            Ok(repo) => {
                debug!("Opened existing repository at {:?}", self.repo_path);
                Ok(repo)
            }
            Err(_) => {
                info!("Initializing new git repository at {:?}", self.repo_path);
                Repository::init(&self.repo_path)
                    .map_err(|e| LaszooError::Git(e))
            }
        }
    }
    
    /// Get the status of the repository
    pub fn get_status(&self) -> Result<Vec<(PathBuf, Status)>> {
        let repo = self.init_repo()?;
        let mut status_options = StatusOptions::new();
        status_options.include_untracked(true);
        
        let statuses = repo.statuses(Some(&mut status_options))?;
        let mut results = Vec::new();
        
        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                let path = self.repo_path.join(path);
                let status = entry.status();
                results.push((path, status));
            }
        }
        
        Ok(results)
    }
    
    /// Stage files for commit
    pub fn stage_files(&self, files: &[PathBuf]) -> Result<()> {
        let repo = self.init_repo()?;
        let mut index = repo.index()?;
        
        for file in files {
            // Get relative path from repo root
            let relative_path = file.strip_prefix(&self.repo_path)
                .unwrap_or(file);
                
            index.add_path(relative_path)?;
            debug!("Staged file: {:?}", relative_path);
        }
        
        index.write()?;
        Ok(())
    }
    
    /// Stage all changes
    pub fn stage_all(&self) -> Result<()> {
        let repo = self.init_repo()?;
        let mut index = repo.index()?;
        
        index.add_all(&["."], IndexAddOption::DEFAULT, None)?;
        index.write()?;
        
        info!("Staged all changes");
        Ok(())
    }
    
    /// Create a commit with an AI-generated message (with fallback to generic message)
    pub async fn commit_with_ai(
        &self,
        ollama_endpoint: &str,
        ollama_model: &str,
        user_context: Option<&str>,
    ) -> Result<Oid> {
        let repo = self.init_repo()?;
        
        // Get diff for staged changes
        let diff_text = self.get_staged_diff()?;
        
        if diff_text.is_empty() {
            return Err(LaszooError::Other("No staged changes to commit".to_string()));
        }
        
        // Try to generate commit message using Ollama, fall back to generic if it fails
        let commit_message = match self.generate_commit_message(
            ollama_endpoint,
            ollama_model,
            &diff_text,
            user_context
        ).await {
            Ok(message) => message,
            Err(e) => {
                warn!("Failed to generate AI commit message: {}. Using generic message.", e);
                self.generate_generic_commit_message(&diff_text, user_context)
            }
        };
        
        // Create the commit
        let signature = self.get_signature()?;
        let tree_id = {
            let mut index = repo.index()?;
            index.write_tree()?
        };
        
        let tree = repo.find_tree(tree_id)?;
        let parent_commit = self.get_head_commit(&repo).ok();
        
        let commit_id = match parent_commit {
            Some(parent) => {
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    &commit_message,
                    &tree,
                    &[&parent],
                )?
            }
            None => {
                // First commit
                repo.commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    &commit_message,
                    &tree,
                    &[],
                )?
            }
        };
        
        info!("Created commit: {}", commit_id);
        println!("\nCommit message:\n{}", commit_message);
        
        Ok(commit_id)
    }
    
    /// Get staged diff
    fn get_staged_diff(&self) -> Result<String> {
        let repo = self.init_repo()?;
        let head = self.get_head_commit(&repo).ok();
        
        let diff = match head {
            Some(commit) => {
                let tree = commit.tree()?;
                let index = repo.index()?;
                repo.diff_tree_to_index(Some(&tree), Some(&index), None)?
            }
            None => {
                // No commits yet, diff against empty tree
                let index = repo.index()?;
                repo.diff_tree_to_index(None, Some(&index), None)?
            }
        };
        
        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let content = std::str::from_utf8(line.content()).unwrap_or("");
            diff_text.push_str(content);
            true
        })?;
        
        Ok(diff_text)
    }
    
    /// Generate commit message using Ollama
    async fn generate_commit_message(
        &self,
        endpoint: &str,
        model: &str,
        diff: &str,
        user_context: Option<&str>,
    ) -> Result<String> {
        let client = reqwest::Client::new();
        
        // Truncate diff if too long
        let max_diff_length = 4000;
        let truncated_diff = if diff.len() > max_diff_length {
            format!("{}... (truncated)", &diff[..max_diff_length])
        } else {
            diff.to_string()
        };
        
        let context = user_context.unwrap_or("");
        let prompt = format!(
            "Generate a concise git commit message for the following changes. \
            Follow conventional commit format (type: description). \
            Include a brief summary line (50 chars or less) and optional body. \
            Context: {}\n\nChanges:\n{}\n\nCommit message:",
            context, truncated_diff
        );
        
        let request = OllamaRequest {
            model: model.to_string(),
            prompt,
            stream: false,
        };
        
        debug!("Sending request to Ollama at {}", endpoint);
        
        let response = client
            .post(format!("{}/api/generate", endpoint))
            .json(&request)
            .send()
            .await
            .map_err(|e| LaszooError::Http(e))?;
            
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(LaszooError::Other(
                format!("Ollama request failed with status {}: {}", status, text)
            ));
        }
        
        let ollama_response: OllamaResponse = response.json().await
            .map_err(|e| LaszooError::Http(e))?;
            
        // Clean up the response - remove thinking tags if present
        let mut message = ollama_response.response.trim().to_string();
        
        // Remove <think> tags if present
        if let Some(start) = message.find("<think>") {
            if let Some(end) = message.find("</think>") {
                let before = message[..start].to_string();
                let after = message[end + 8..].to_string();
                message = format!("{}{}", before, after);
            }
        }
        
        let message = message.trim();
        
        // Add Laszoo attribution
        let final_message = format!("{}\n\n🦎 Laszoo: AI-generated commit message", message);
        
        Ok(final_message)
    }
    
    /// Generate a generic commit message based on diff analysis
    fn generate_generic_commit_message(&self, diff: &str, user_context: Option<&str>) -> String {
        let mut added_files = 0;
        let mut modified_files = 0;
        let mut deleted_files = 0;
        let mut added_lines = 0;
        let mut deleted_lines = 0;
        
        // Parse the diff to understand what changed
        for line in diff.lines() {
            if line.starts_with("diff --git") {
                // Count file modifications
                if line.contains("/dev/null") {
                    if line.starts_with("diff --git a/") {
                        deleted_files += 1;
                    } else {
                        added_files += 1;
                    }
                } else {
                    modified_files += 1;
                }
            } else if line.starts_with("+") && !line.starts_with("+++") {
                added_lines += 1;
            } else if line.starts_with("-") && !line.starts_with("---") {
                deleted_lines += 1;
            }
        }
        
        // Generate appropriate commit message based on changes
        let message = if user_context.is_some() && !user_context.unwrap().is_empty() {
            user_context.unwrap().to_string()
        } else if added_files > 0 && modified_files == 0 && deleted_files == 0 {
            if added_files == 1 {
                "feat: Add new file"
            } else {
                "feat: Add new files"
            }.to_string()
        } else if deleted_files > 0 && added_files == 0 && modified_files == 0 {
            if deleted_files == 1 {
                "chore: Remove file"
            } else {
                "chore: Remove files"
            }.to_string()
        } else if modified_files > 0 && added_files == 0 && deleted_files == 0 {
            if modified_files == 1 {
                "feat: Update configuration"
            } else {
                "feat: Update configurations"
            }.to_string()
        } else {
            // Mixed changes
            let mut parts = Vec::new();
            if added_files > 0 {
                parts.push(format!("{} added", added_files));
            }
            if modified_files > 0 {
                parts.push(format!("{} modified", modified_files));
            }
            if deleted_files > 0 {
                parts.push(format!("{} deleted", deleted_files));
            }
            
            if parts.is_empty() {
                "feat: Update files".to_string()
            } else {
                format!("feat: Update files ({})", parts.join(", "))
            }
        };
        
        // Add line change statistics if significant
        let mut stats = Vec::new();
        if added_lines > 0 {
            stats.push(format!("+{}", added_lines));
        }
        if deleted_lines > 0 {
            stats.push(format!("-{}", deleted_lines));
        }
        
        let final_message = if !stats.is_empty() && (added_lines + deleted_lines) > 5 {
            format!("{}\n\n({} lines changed)", message, stats.join("/"))
        } else {
            message
        };
        
        format!("{}\n\n🦎 Laszoo: Auto-generated commit message", final_message)
    }
    
    /// Get git signature
    fn get_signature(&self) -> Result<Signature<'static>> {
        let repo = self.init_repo()?;
        let config = repo.config()?;
        
        let name = config.get_string("user.name")
            .unwrap_or_else(|_| "Laszoo User".to_string());
        let email = config.get_string("user.email")
            .unwrap_or_else(|_| "laszoo@localhost".to_string());
            
        Signature::now(&name, &email)
            .map_err(|e| LaszooError::Git(e))
    }
    
    /// Get HEAD commit
    fn get_head_commit<'a>(&self, repo: &'a Repository) -> Result<git2::Commit<'a>> {
        let head = repo.head()?;
        let oid = head.target()
            .ok_or_else(|| LaszooError::Other("HEAD has no target".to_string()))?;
        let commit = repo.find_commit(oid)?;
        Ok(commit)
    }
    
    /// Check if there are uncommitted changes
    pub fn has_changes(&self) -> Result<bool> {
        let statuses = self.get_status()?;
        Ok(!statuses.is_empty())
    }
}