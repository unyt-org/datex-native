use crate::workbench::views::comhub::ComHub;
use crate::workbench::views::metadata::Metadata;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use datex_core::runtime::Runtime;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::{
    DefaultTerminal, Frame, layout::Rect, style::Stylize, text::Line, widgets::Paragraph,
};
use std::io;
use std::time::Duration;
use tokio::task::yield_now;

pub struct Workbench {
    runtime: Runtime,
    metadata: Metadata,
    comhub: ComHub,
    exit: bool,
}

impl Workbench {
    pub fn new(runtime: Runtime) -> Workbench {
        // init the views
        let metadata = Metadata {
            runtime: runtime.clone(),
        };
        let comhub = ComHub {
            runtime: runtime.clone(),
        };

        Workbench {
            runtime,
            metadata,
            comhub,
            exit: false,
        }
    }

    /// runs the application's main loop until the user quits
    pub async fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;

            // // add ptr to the runtime
            // let id = random_bytes_slice::<26>();
            // runtime
            //     .memory
            //     .borrow_mut()
            //     .store_pointer(id, Pointer::from_id(id.to_vec()));

            yield_now().await; // let other tasks run
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![
                Constraint::Length(1),
                Constraint::Percentage(20),
                Constraint::Min(10),
            ])
            .split(frame.area());

        // draw the title
        self.draw_title(frame, layout[0]);

        // draw views
        frame.render_widget(&self.metadata, layout[1]);
        frame.render_widget(&self.comhub, layout[2]);
    }

    fn draw_title(&self, frame: &mut Frame, area: Rect) {
        let title = Line::from(vec![
            " DATEX Workbench ".bold(),
            format!("v{} ", self.runtime.version).dim(),
        ])
        .black();

        frame.render_widget(Paragraph::new(title).on_white(), area);
    }

    fn handle_events(&mut self) -> io::Result<()> {
        if let Ok(true) = event::poll(Duration::from_millis(10)) {
            match event::read()? {
                Event::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    self.handle_key_event(key_event)
                }
                _ => {}
            };
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key_event: KeyEvent) {
        if let KeyCode::Char('q') = key_event.code {
            self.exit()
        }
    }

    fn exit(&mut self) {
        self.exit = true;
    }
}
