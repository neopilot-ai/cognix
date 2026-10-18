use clap::Parser;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = cognix_terminal::cli::Cli::parse();
    let code = cognix_terminal::main(cli).await;
    std::process::ExitCode::from(code.clamp(0, 255) as u8)
}