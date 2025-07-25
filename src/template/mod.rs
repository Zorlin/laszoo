use std::path::Path;
use std::collections::HashMap;
use handlebars::Handlebars;
use regex::Regex;
use serde_json::Value;
use tracing::{debug, warn};
use crate::error::{LaszooError, Result};

pub struct TemplateEngine {
    handlebars: Handlebars<'static>,
    quack_regex: Regex,
}

impl TemplateEngine {
    pub fn new() -> Result<Self> {
        let mut handlebars = Handlebars::new();
        handlebars.set_strict_mode(false);
        
        // Regex for quack tags: [[x content x]]
        let quack_regex = Regex::new(r"\[\[x\s*(.*?)\s*x\]\]")
            .map_err(|e| LaszooError::Template(format!("Invalid regex: {}", e)))?;
        
        Ok(Self {
            handlebars,
            quack_regex,
        })
    }
    
    /// Process a template file with variables and quack tags
    pub fn process_template(
        &self,
        template_content: &str,
        variables: &HashMap<String, Value>,
        preserve_quack_tags: bool,
    ) -> Result<String> {
        let mut content = template_content.to_string();
        
        // If preserving quack tags, extract them before Handlebars processing
        let mut quack_placeholders = HashMap::new();
        if preserve_quack_tags {
            let mut placeholder_id = 0;
            content = self.quack_regex.replace_all(&content, |caps: &regex::Captures| {
                let quack_content = caps.get(1).map_or("", |m| m.as_str());
                let placeholder = format!("__QUACK_PLACEHOLDER_{}__", placeholder_id);
                quack_placeholders.insert(placeholder.clone(), format!("[[x {} x]]", quack_content));
                placeholder_id += 1;
                placeholder
            }).to_string();
        }
        
        // Process Handlebars template
        let rendered = self.handlebars.render_template(&content, variables)
            .map_err(|e| LaszooError::Template(format!("Handlebars error: {}", e)))?;
        
        // Restore quack tags if preserved
        let mut final_content = rendered;
        if preserve_quack_tags {
            for (placeholder, original) in quack_placeholders {
                final_content = final_content.replace(&placeholder, &original);
            }
        }
        
        Ok(final_content)
    }
    
    /// Extract quack tags from a template
    pub fn extract_quack_tags(&self, template_content: &str) -> Vec<QuackTag> {
        self.quack_regex.captures_iter(template_content)
            .enumerate()
            .map(|(index, caps)| {
                let content = caps.get(1).map_or("", |m| m.as_str()).to_string();
                let full_match = caps.get(0).unwrap();
                QuackTag {
                    id: index,
                    content: content.trim().to_string(),
                    start: full_match.start(),
                    end: full_match.end(),
                }
            })
            .collect()
    }
    
    /// Compare two templates and identify divergences
    pub fn compare_templates(
        &self,
        template1: &str,
        template2: &str,
    ) -> TemplateComparison {
        let tags1 = self.extract_quack_tags(template1);
        let tags2 = self.extract_quack_tags(template2);
        
        // Strip quack tags for content comparison
        let content1 = self.quack_regex.replace_all(template1, "").to_string();
        let content2 = self.quack_regex.replace_all(template2, "").to_string();
        
        let content_matches = content1 == content2;
        let tags_match = tags1.len() == tags2.len() && 
                        tags1.iter().zip(&tags2).all(|(t1, t2)| t1.content == t2.content);
        
        TemplateComparison {
            content_matches,
            tags_match,
            tags1,
            tags2,
        }
    }
    
    /// Create a merged template from multiple divergent templates
    pub fn merge_templates(
        &self,
        templates: Vec<(&str, &str)>, // (hostname, template_content)
    ) -> Result<MergedTemplate> {
        if templates.is_empty() {
            return Err(LaszooError::Template("No templates to merge".to_string()));
        }
        
        // Find the most common template (majority)
        let mut template_counts: HashMap<String, Vec<String>> = HashMap::new();
        
        for (hostname, content) in &templates {
            let normalized = self.quack_regex.replace_all(content, "").to_string();
            template_counts.entry(normalized)
                .or_insert_with(Vec::new)
                .push(hostname.to_string());
        }
        
        // Find majority template
        let (base_content, majority_hosts) = template_counts.into_iter()
            .max_by_key(|(_, hosts)| hosts.len())
            .unwrap();
        
        // Collect all unique quack tags
        let mut all_quack_tags: HashMap<String, Vec<String>> = HashMap::new();
        for (hostname, content) in &templates {
            let tags = self.extract_quack_tags(content);
            for tag in tags {
                all_quack_tags.entry(tag.content.clone())
                    .or_insert_with(Vec::new)
                    .push(hostname.to_string());
            }
        }
        
        // Create merged template with all quack tags
        let mut merged_content = base_content;
        for (tag_content, hosts) in &all_quack_tags {
            if hosts.len() < templates.len() {
                // This is a divergent section
                let quack_tag = format!("[[x {} x]]", tag_content);
                merged_content.push_str(&format!("\n{}", quack_tag));
            }
        }
        
        Ok(MergedTemplate {
            content: merged_content,
            majority_hosts,
            divergent_sections: all_quack_tags.into_iter()
                .filter(|(_, hosts)| hosts.len() < templates.len())
                .collect(),
        })
    }
}

/// Process template with handlebars variables only
pub fn process_handlebars(template_content: &str, hostname: &str) -> Result<String> {
    let engine = TemplateEngine::new()?;
    let mut vars = HashMap::new();
    vars.insert("hostname".to_string(), serde_json::json!(hostname));
    
    // Add more system variables as needed
    engine.process_template(template_content, &vars, false)
}

/// Process template with quack tags from machine-specific content
pub fn process_with_quacks(group_template: &str, machine_template: &str) -> Result<String> {
    let engine = TemplateEngine::new()?;
    
    // Extract quack tags from machine template
    let machine_quacks = engine.extract_quack_tags(machine_template);
    
    // Replace {{ quack }} placeholders in group template with machine-specific content
    let mut result = group_template.to_string();
    let quack_placeholder_regex = Regex::new(r"\{\{\s*quack\s*\}\}")?;
    
    for (i, caps) in quack_placeholder_regex.find_iter(group_template).enumerate() {
        if let Some(quack_tag) = machine_quacks.get(i) {
            result = result.replacen(caps.as_str(), &quack_tag.content, 1);
        }
    }
    
    Ok(result)
}

#[derive(Debug, Clone)]
pub struct QuackTag {
    pub id: usize,
    pub content: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug)]
pub struct TemplateComparison {
    pub content_matches: bool,
    pub tags_match: bool,
    pub tags1: Vec<QuackTag>,
    pub tags2: Vec<QuackTag>,
}

#[derive(Debug)]
pub struct MergedTemplate {
    pub content: String,
    pub majority_hosts: Vec<String>,
    pub divergent_sections: HashMap<String, Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_quack_tag_extraction() {
        let engine = TemplateEngine::new().unwrap();
        let template = r#"
server {
    port = 8080
    [[x host = "production.example.com" x]]
    [[x debug = true x]]
}
"#;
        
        let tags = engine.extract_quack_tags(template);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].content, "host = \"production.example.com\"");
        assert_eq!(tags[1].content, "debug = true");
    }
    
    #[test]
    fn test_template_processing() {
        let engine = TemplateEngine::new().unwrap();
        let template = r#"
server {
    port = {{port}}
    host = "{{hostname}}"
    [[x debug = true x]]
}
"#;
        
        let mut vars = HashMap::new();
        vars.insert("port".to_string(), serde_json::json!(8080));
        vars.insert("hostname".to_string(), serde_json::json!("example.com"));
        
        let result = engine.process_template(template, &vars, true).unwrap();
        assert!(result.contains("port = 8080"));
        assert!(result.contains("host = \"example.com\""));
        assert!(result.contains("[[x debug = true x]]"));
    }
}