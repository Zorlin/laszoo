use std::collections::HashMap;
use std::path::PathBuf;

fn main() {
    let mut enrollments: HashMap<PathBuf, String> = HashMap::new();
    
    enrollments.insert(PathBuf::from("/tmp/test/file1.txt"), "entry1".to_string());
    enrollments.insert(PathBuf::from("/tmp/test/file2.txt"), "entry2".to_string());
    
    println!("HashMap has {} entries", enrollments.len());
    
    // Test 1: iter() directly
    println!("\nTest 1: Using iter() directly:");
    for (path, entry) in enrollments.iter() {
        println!("  Path: {}, Entry: {}", path.display(), entry);
    }
    
    // Test 2: into_iter() with collect
    println!("\nTest 2: Using into_iter() with collect:");
    let entries: Vec<_> = enrollments.into_iter().collect();
    println!("Vec has {} entries", entries.len());
    for (path, entry) in entries {
        println!("  Path: {}, Entry: {}", path.display(), entry);
    }
}