use crate::workbench::workbench::Workbench;
use datex_core::runtime::Runtime;
use std::io;

mod views;
mod workbench;

pub async fn start_workbench(runtime: Runtime) -> io::Result<()> {
    let mut terminal = ratatui::init();
    let app_result = Workbench::new(runtime).run(&mut terminal).await;
    ratatui::restore();
    app_result
}
