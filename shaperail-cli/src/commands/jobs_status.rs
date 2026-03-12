/// Show job queue depth and recent failures.
pub fn run() -> i32 {
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    println!("Job Queue Status");
    println!("================");
    println!("Redis: {redis_url}");
    println!();

    // Try to connect to Redis and show queue status
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create runtime: {e}");
            return 1;
        }
    };

    rt.block_on(async {
        match check_redis_queues(&redis_url).await {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("Failed to connect to Redis: {e}");
                eprintln!("Is Redis running? Start with: docker compose up -d redis");
                1
            }
        }
    })
}

async fn check_redis_queues(redis_url: &str) -> Result<(), String> {
    let client = redis::Client::open(redis_url).map_err(|e| format!("Invalid Redis URL: {e}"))?;
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| format!("Connection failed: {e}"))?;

    let queues = ["critical", "high", "normal", "low"];

    println!("{:<12} DEPTH", "QUEUE");
    println!("{}", "-".repeat(30));

    for queue in &queues {
        let key = format!("shaperail:jobs:queue:{queue}");
        let len: i64 = redis::cmd("LLEN")
            .arg(&key)
            .query_async(&mut conn)
            .await
            .unwrap_or(0);
        println!("{:<12} {}", queue, len);
    }

    // Dead letter queue
    let dead_len: i64 = redis::cmd("LLEN")
        .arg("shaperail:jobs:dead")
        .query_async(&mut conn)
        .await
        .unwrap_or(0);

    println!();
    println!("Dead letter queue: {dead_len} job(s)");

    Ok(())
}
