//! The procedures tool's executable: one resource request handed by the tool host on standard input, answered
//! on standard output (docs/tools.md, 「A tool's folder」).

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
  tool_call::answer_standard_io(procedures_tool::answer).await
}
