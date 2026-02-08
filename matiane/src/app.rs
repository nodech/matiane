use iced::alignment;
use iced::widget::{
    Theme, button, column, container, pick_list, row, rule, space, text,
    tooltip,
};
use std::collections::BTreeSet;

use chrono::TimeZone;
use iced::{Element, Fill, Subscription, Task};

use crate::config;
use crate::icon;

const DEFAULT_LIGHT: Theme = Theme::Light;
const DEFAULT_DARK: Theme = Theme::Nord;

#[derive(Default, Debug)]
pub enum Screen {
    #[default]
    Initial,
}

#[derive(Default, Debug)]
pub enum State {
    #[default]
    Loading,
    Initialized,
}

#[derive(Debug, Clone)]
pub enum Message {
    LoadedDates(()),
    ThemeToggle,
}

#[derive(Debug)]
pub struct App {
    screen: Screen,
    theme: Theme,
    state: State,

    config: config::MatianeConfig,
    tz_offset: chrono::FixedOffset,
    loaded_dates: Option<()>,
}

impl App {
    pub fn new(cfg: config::MatianeConfig) -> (Self, Task<Message>) {
        let tz_offset = *chrono::Local::now().offset();

        (
            App {
                theme: DEFAULT_DARK,
                screen: Screen::default(),
                state: State::default(),

                config: cfg,
                tz_offset,
                loaded_dates: None,
            },
            Task::none(),
        )
    }

    pub fn title(&self) -> String {
        "Matiane".into()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::LoadedDates(dates) => {
                self.loaded_dates = Some(dates);
            }
            Message::ThemeToggle => {
                if self.theme == DEFAULT_LIGHT {
                    self.theme = DEFAULT_DARK
                } else {
                    self.theme = DEFAULT_LIGHT
                }
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = match self.state {
            State::Loading => loading(),
            State::Initialized => nothing_space(),
        };

        let out: Element<'_, Message> = column![
            self.view_header(),
            rule::horizontal(2).style(rule::weak),
            content
        ]
        .into();

        // out.explain(iced::Color::from_rgb(1.0, 0.0, 0.0))
        out
    }

    pub fn view_header(&self) -> Element<'_, Message> {
        let logo_name = container("Matiane")
            .padding(iced::Padding {
                left: 30.0,
                ..Default::default()
            })
            .align_y(iced::Alignment::Center)
            .height(50);

        let timezone = tooltip(
            row![
                container(icon::globe().width(Fill).height(Fill).center())
                    .width(40)
                    .height(Fill)
                    .align_x(alignment::Horizontal::Right)
                    .align_y(iced::Alignment::Center),
                text(self.tz_offset.to_string())
                    .height(Fill)
                    .align_y(iced::Alignment::Center),
            ],
            container("Timezone")
                .padding(10)
                .style(container::rounded_box),
            tooltip::Position::Bottom,
        );

        let theme_switch = tooltip(
            button(icon::moon().height(Fill).center())
                .padding(10)
                .on_press(Message::ThemeToggle),
            container(if self.theme == DEFAULT_DARK {
                "Switch to light mode"
            } else {
                "Switch to dark mode"
            })
            .padding(10)
            .style(container::rounded_box),
            tooltip::Position::Bottom,
        );

        let header = row![
            logo_name,
            container(row![
                container(timezone).padding(10),
                container(theme_switch).padding(10),
            ])
            .width(Fill)
            .align_x(alignment::Horizontal::Right)
            .align_y(iced::Alignment::Center)
            .padding(iced::Padding {
                right: 30.0,
                ..Default::default()
            })
            .height(50),
        ];

        header.into()
    }

    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
    }
}

pub fn loading<'a, Message: 'a>() -> Element<'a, Message> {
    container("Loading...")
        .width(Fill)
        .height(Fill)
        .align_x(iced::Alignment::Center)
        .align_y(iced::Alignment::Center)
        .into()
}

pub fn nothing_space<'a, Message: 'a>() -> Element<'a, Message> {
    space().into()
}
