#[tokio::main]
async fn main() {
    #[cfg(feature = "debug_dhat")]
    let _profiler = datex::core::dhat::Profiler::new_heap();
    datex_cli::run().await;
}
