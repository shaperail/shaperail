/// Print all routes with auth requirements.
pub fn run() -> i32 {
    let resources = match super::load_all_resources() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };

    println!("{:<8} {:<30} AUTH", "METHOD", "PATH");
    println!("{}", "-".repeat(70));

    for resource in &resources {
        if let Some(endpoints) = &resource.endpoints {
            for (_action, ep) in endpoints {
                let method = ep.method().to_string();
                let auth_str = match &ep.auth {
                    Some(rule) => format!("{rule}"),
                    None => "public".to_string(),
                };

                let versioned_path = format!("/v{}{}", resource.version, ep.path());
                println!("{:<8} {:<30} {}", method, versioned_path, auth_str);
            }
        }
    }

    0
}
