use datex_core::network::com_interfaces::com_interface_properties::InterfaceDirection;
use datex_core::runtime::Runtime;
use ratatui::style::{Color, Style};
use ratatui::widgets::Borders;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Stylize,
    text::{Line, Text},
    widgets::{Block, Paragraph, Widget},
};
pub struct ComHub {
    pub runtime: Runtime,
}

impl Widget for &ComHub {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let metadata = self.runtime.com_hub().get_metadata();

        let block = Block::default()
            .title(" ComHub ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White));

        let mut lines = vec![
            Line::from(vec![
                "Registered Interfaces: ".into(),
                metadata.interfaces.len().to_string().into(),
            ]),
            Line::from(vec![
                "Connected Sockets: ".into(),
                metadata.endpoint_sockets.len().to_string().into(),
            ]),
        ];

        // add newline
        lines.push(Line::from(vec!["".into()]));

        // iterate interfaces
        for interface in metadata.interfaces.iter() {
            lines.push(Line::from(vec![
                match &interface.properties.name {
                    Some(name) => format!("{} ({})", interface.properties.channel, name),
                    None => interface.properties.channel.to_string(),
                }
                .to_string()
                .bold(),
            ]));

            // iterate sockets
            for socket in interface.sockets.iter() {
                lines.push(Line::from(vec![
                    "  ⬤ ".to_string().green(),
                    match socket.direction {
                        InterfaceDirection::In => " ──▶ ".to_string().into(),
                        InterfaceDirection::Out => " ◀── ".to_string().into(),
                        InterfaceDirection::InOut => " ◀──▶ ".to_string().into(),
                    },
                    (match &socket.endpoint {
                        Some(endpoint) => endpoint.to_string(),
                        None => "unknown".to_string(),
                    })
                    .to_string()
                    .into(),
                ]));
            }
        }

        Paragraph::new(Text::from_iter(lines))
            .block(block)
            .render(area, buf);
    }
}
