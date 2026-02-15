use std::cell::RefCell;

use wasm_bindgen::{closure::Closure, JsCast, JsValue};
use web_sys::{Document, Element, Event, HtmlElement, HtmlInputElement, HtmlSelectElement, KeyboardEvent};

use crate::core::{Game, GameStatus};
use crate::difficulty::{validate_custom, DifficultyPreset, DifficultySettings};
use crate::persistence;

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let app = App::new()?;
    APP.with(|slot| {
        *slot.borrow_mut() = Some(app);
    });

    with_app_mut(|app| {
        app.attach_event_listeners()?;
        app.start_timer()?;
        app.render_all()
    })
    .transpose()?
    .ok_or_else(|| JsValue::from_str("Application state missing"))?;

    Ok(())
}

fn with_app_mut<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut App) -> R,
{
    APP.with(|slot| {
        let mut borrow = slot.borrow_mut();
        let app = borrow.as_mut()?;
        Some(f(app))
    })
}

#[derive(Clone, Debug)]
struct DifficultyChoice {
    settings: DifficultySettings,
    best_key: String,
    storage_value: String,
}

struct App {
    document: Document,
    board: HtmlElement,
    status: HtmlElement,
    status_emoji: HtmlElement,
    mine_counter: HtmlElement,
    timer_counter: HtmlElement,
    best_counter: HtmlElement,
    difficulty_select: HtmlSelectElement,
    custom_settings: HtmlElement,
    custom_width: HtmlInputElement,
    custom_height: HtmlInputElement,
    custom_mines: HtmlInputElement,
    new_game_button: HtmlElement,
    theme_toggle: HtmlElement,
    theme_toggle_icon: HtmlElement,
    game: Game,
    is_dark: bool,
    difficulty_choice: DifficultyChoice,
    best_time_seconds: Option<u64>,
    event_handlers: Vec<Closure<dyn FnMut(Event)>>,
    timer_handler: Option<Closure<dyn FnMut()>>,
    timer_id: Option<i32>,
    cursor_x: usize,
    cursor_y: usize,
}

impl App {
    fn new() -> Result<Self, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("Window unavailable"))?;
        let document = window
            .document()
            .ok_or_else(|| JsValue::from_str("Document unavailable"))?;

        let board = by_id::<HtmlElement>(&document, "board")?;
        let status = by_id::<HtmlElement>(&document, "status")?;
        let status_emoji = by_id::<HtmlElement>(&document, "status-emoji")?;
        let mine_counter = by_id::<HtmlElement>(&document, "mine-counter")?;
        let timer_counter = by_id::<HtmlElement>(&document, "time-counter")?;
        let best_counter = by_id::<HtmlElement>(&document, "best-counter")?;
        let difficulty_select = by_id::<HtmlSelectElement>(&document, "difficulty")?;
        let custom_settings = by_id::<HtmlElement>(&document, "custom-settings")?;
        let custom_width = by_id::<HtmlInputElement>(&document, "custom-width")?;
        let custom_height = by_id::<HtmlInputElement>(&document, "custom-height")?;
        let custom_mines = by_id::<HtmlInputElement>(&document, "custom-mines")?;
        let new_game_button = by_id::<HtmlElement>(&document, "new-game")?;
        let theme_toggle = by_id::<HtmlElement>(&document, "theme-toggle")?;
        let theme_toggle_icon = by_id::<HtmlElement>(&document, "theme-toggle-icon")?;

        let initial_choice = parse_saved_choice(persistence::load_difficulty().as_deref())
            .unwrap_or_else(|| preset_choice(DifficultyPreset::Beginner));

        apply_choice_to_controls(
            &difficulty_select,
            &custom_width,
            &custom_height,
            &custom_mines,
            &initial_choice,
        );

        let best_time_seconds = persistence::load_best_time_seconds(&initial_choice.best_key);

        let is_dark = persistence::load_theme().as_deref() != Some("light");
        if let Some(root) = document.document_element() {
            if is_dark {
                let _ = root.remove_attribute("data-theme");
            } else {
                let _ = root.set_attribute("data-theme", "light");
            }
        }

        Ok(Self {
            document,
            board,
            status,
            status_emoji,
            mine_counter,
            timer_counter,
            best_counter,
            difficulty_select,
            custom_settings,
            custom_width,
            custom_height,
            custom_mines,
            new_game_button,
            theme_toggle,
            theme_toggle_icon,
            game: Game::new(initial_choice.settings.clone()),
            is_dark,
            difficulty_choice: initial_choice,
            best_time_seconds,
            event_handlers: Vec::new(),
            timer_handler: None,
            timer_id: None,
            cursor_x: 0,
            cursor_y: 0,
        })
    }

    fn attach_event_listeners(&mut self) -> Result<(), JsValue> {
        let board_click = Closure::wrap(Box::new(move |event: Event| {
            if let Some((x, y)) = event_coords(&event) {
                let _ = with_app_mut(|app| {
                    app.handle_primary_click(x, y);
                });
            }
        }) as Box<dyn FnMut(Event)>);
        self.board
            .add_event_listener_with_callback("click", board_click.as_ref().unchecked_ref())?;
        self.event_handlers.push(board_click);

        let board_context = Closure::wrap(Box::new(move |event: Event| {
            event.prevent_default();
            if let Some((x, y)) = event_coords(&event) {
                let _ = with_app_mut(|app| {
                    app.set_cursor(x, y);
                    app.handle_toggle_flag(x, y);
                });
            }
        }) as Box<dyn FnMut(Event)>);
        self.board.add_event_listener_with_callback(
            "contextmenu",
            board_context.as_ref().unchecked_ref(),
        )?;
        self.event_handlers.push(board_context);

        let difficulty_change = Closure::wrap(Box::new(move |_event: Event| {
            let _ = with_app_mut(|app| {
                let _ = app.sync_custom_visibility();
            });
        }) as Box<dyn FnMut(Event)>);
        self.difficulty_select.add_event_listener_with_callback(
            "change",
            difficulty_change.as_ref().unchecked_ref(),
        )?;
        self.event_handlers.push(difficulty_change);

        let new_game = Closure::wrap(Box::new(move |_event: Event| {
            let _ = with_app_mut(|app| {
                app.start_new_game();
            });
        }) as Box<dyn FnMut(Event)>);
        self.new_game_button
            .add_event_listener_with_callback("click", new_game.as_ref().unchecked_ref())?;
        self.event_handlers.push(new_game);

        let keyboard = Closure::wrap(Box::new(move |event: Event| {
            let Ok(key_event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };

            if should_ignore_key_event(&key_event) {
                return;
            }

            let handled = with_app_mut(|app| app.handle_key_event(&key_event)).unwrap_or(false);
            if handled {
                key_event.prevent_default();
            }
        }) as Box<dyn FnMut(Event)>);
        self.document
            .add_event_listener_with_callback("keydown", keyboard.as_ref().unchecked_ref())?;
        self.event_handlers.push(keyboard);

        let theme_click = Closure::wrap(Box::new(move |_event: Event| {
            let _ = with_app_mut(|app| {
                app.toggle_theme();
            });
        }) as Box<dyn FnMut(Event)>);
        self.theme_toggle
            .add_event_listener_with_callback("click", theme_click.as_ref().unchecked_ref())?;
        self.event_handlers.push(theme_click);

        self.sync_custom_visibility()?;
        self.render_theme_icon();

        Ok(())
    }

    fn toggle_theme(&mut self) {
        self.is_dark = !self.is_dark;
        if let Some(root) = self.document.document_element() {
            if self.is_dark {
                let _ = root.remove_attribute("data-theme");
            } else {
                let _ = root.set_attribute("data-theme", "light");
            }
        }
        persistence::save_theme(if self.is_dark { "dark" } else { "light" });
        self.render_theme_icon();
    }

    fn render_theme_icon(&self) {
        let icon = if self.is_dark { "\u{2600}\u{FE0F}" } else { "\u{1F319}" };
        self.theme_toggle_icon.set_text_content(Some(icon));
    }

    fn start_timer(&mut self) -> Result<(), JsValue> {
        if self.timer_id.is_some() {
            return Ok(());
        }

        let callback = Closure::wrap(Box::new(move || {
            let _ = with_app_mut(|app| {
                let _ = app.render_timer();
            });
        }) as Box<dyn FnMut()>);

        let window = web_sys::window().ok_or_else(|| JsValue::from_str("Window unavailable"))?;
        let timer_id = window.set_interval_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            100,
        )?;

        self.timer_id = Some(timer_id);
        self.timer_handler = Some(callback);

        Ok(())
    }

    fn start_new_game(&mut self) {
        match self.choice_from_controls() {
            Ok(choice) => {
                self.best_time_seconds = persistence::load_best_time_seconds(&choice.best_key);
                persistence::save_difficulty(&choice.storage_value);
                self.game.reset(choice.settings.clone());
                self.difficulty_choice = choice;
                self.cursor_x = 0;
                self.cursor_y = 0;
                let _ = self.render_all();
            }
            Err(message) => {
                self.status.set_text_content(Some(&message));
            }
        }
    }

    fn handle_primary_click(&mut self, x: usize, y: usize) {
        self.set_cursor(x, y);
        if self.game.cell(x, y).map(|cell| cell.revealed).unwrap_or(false) {
            self.handle_chord(x, y);
        } else {
            self.handle_reveal(x, y);
        }
    }

    fn handle_reveal(&mut self, x: usize, y: usize) {
        let before = self.game.status();
        if !self.game.reveal(x, y, now_ms()) {
            return;
        }

        if before != GameStatus::Won && self.game.status() == GameStatus::Won {
            self.record_best_time();
        }

        let _ = self.render_all();
    }

    fn handle_chord(&mut self, x: usize, y: usize) {
        let before = self.game.status();
        if !self.game.chord_reveal(x, y, now_ms()) {
            return;
        }

        if before != GameStatus::Won && self.game.status() == GameStatus::Won {
            self.record_best_time();
        }

        let _ = self.render_all();
    }

    fn handle_toggle_flag(&mut self, x: usize, y: usize) {
        if self.game.toggle_flag(x, y) {
            let _ = self.render_all();
        }
    }

    fn handle_key_event(&mut self, event: &KeyboardEvent) -> bool {
        let key = event.key();
        match key.as_str() {
            "ArrowUp" | "w" | "W" => {
                self.move_cursor(0, -1);
                let _ = self.render_all();
                true
            }
            "ArrowDown" | "s" | "S" => {
                self.move_cursor(0, 1);
                let _ = self.render_all();
                true
            }
            "ArrowLeft" | "a" | "A" => {
                self.move_cursor(-1, 0);
                let _ = self.render_all();
                true
            }
            "ArrowRight" | "d" | "D" => {
                self.move_cursor(1, 0);
                let _ = self.render_all();
                true
            }
            " " | "Enter" => {
                self.handle_primary_click(self.cursor_x, self.cursor_y);
                true
            }
            "f" | "F" => {
                self.handle_toggle_flag(self.cursor_x, self.cursor_y);
                true
            }
            "c" | "C" => {
                self.handle_chord(self.cursor_x, self.cursor_y);
                true
            }
            "n" | "N" => {
                self.start_new_game();
                true
            }
            "t" | "T" => {
                self.toggle_theme();
                true
            }
            _ => false,
        }
    }

    fn move_cursor(&mut self, dx: i32, dy: i32) {
        let settings = self.game.settings();
        let max_x = settings.width.saturating_sub(1) as i32;
        let max_y = settings.height.saturating_sub(1) as i32;
        let next_x = (self.cursor_x as i32 + dx).clamp(0, max_x);
        let next_y = (self.cursor_y as i32 + dy).clamp(0, max_y);
        self.cursor_x = next_x as usize;
        self.cursor_y = next_y as usize;
    }

    fn set_cursor(&mut self, x: usize, y: usize) {
        let settings = self.game.settings();
        if x < settings.width && y < settings.height {
            self.cursor_x = x;
            self.cursor_y = y;
        }
    }

    fn record_best_time(&mut self) {
        let elapsed_seconds = self.game.elapsed_ms(now_ms()) / 1_000;
        let should_write = self
            .best_time_seconds
            .map(|value| elapsed_seconds < value)
            .unwrap_or(true);

        if should_write {
            self.best_time_seconds = Some(elapsed_seconds);
            persistence::save_best_time_seconds(&self.difficulty_choice.best_key, elapsed_seconds);
        }
    }

    fn render_all(&mut self) -> Result<(), JsValue> {
        self.render_board()?;
        self.render_header()?;
        self.render_timer()
    }

    fn render_header(&self) -> Result<(), JsValue> {
        self.mine_counter
            .set_text_content(Some(&self.game.flags_left().to_string()));

        let (status_text, emoji) = match self.game.status() {
            GameStatus::Ready => ("Ready", "\u{1F60A}"),
            GameStatus::Running => ("Playing", "\u{1F914}"),
            GameStatus::Won => ("You won!", "\u{1F60E}"),
            GameStatus::Lost => ("Game over", "\u{1F635}"),
        };
        self.status.set_text_content(Some(status_text));
        self.status_emoji.set_text_content(Some(emoji));

        let best = self
            .best_time_seconds
            .map(|seconds| format!("{seconds}s"))
            .unwrap_or_else(|| "--".to_string());
        self.best_counter.set_text_content(Some(&best));

        Ok(())
    }

    fn render_timer(&self) -> Result<(), JsValue> {
        let elapsed_ms = self.game.elapsed_ms(now_ms());
        let text = match self.game.status() {
            GameStatus::Running => format!("{:.1}s", elapsed_ms as f64 / 1_000.0),
            _ => format!("{}s", elapsed_ms / 1_000),
        };
        self.timer_counter.set_text_content(Some(&text));
        Ok(())
    }

    fn render_board(&self) -> Result<(), JsValue> {
        let settings = self.game.settings();
        let game_status = self.game.status();
        self.board.set_inner_html("");
        self.board.set_attribute(
            "style",
            &format!(
                "grid-template-columns: repeat({}, var(--cell-size));",
                settings.width
            ),
        )?;

        for y in 0..settings.height {
            for x in 0..settings.width {
                let cell = self
                    .game
                    .cell(x, y)
                    .ok_or_else(|| JsValue::from_str("Cell out of bounds"))?;

                let button = self.document.create_element("button")?;
                button.set_attribute("type", "button")?;
                button.set_attribute("data-x", &x.to_string())?;
                button.set_attribute("data-y", &y.to_string())?;

                let mut classes = vec!["cell"];
                let mut label = String::with_capacity(4);

                if cell.revealed {
                    classes.push("revealed");
                    if cell.mine {
                        classes.push("mine");
                        label.push_str("\u{1F4A3}");
                        if game_status == GameStatus::Lost {
                            classes.push("mine-sweep");
                            let delay_ms = (x + y) * 40;
                            button.set_attribute(
                                "style",
                                &format!("animation-delay:{}ms", delay_ms),
                            )?;
                        }
                    } else if cell.adjacent > 0 {
                        classes.push("number");
                        classes.push(number_class(cell.adjacent));
                        label = cell.adjacent.to_string();
                    }
                } else if cell.flagged {
                    classes.push("flagged");
                    label.push_str("\u{1F6A9}");
                    if game_status == GameStatus::Won && cell.mine {
                        classes.push("flag-sweep");
                        let delay_ms = (x + y) * 40;
                        button.set_attribute(
                            "style",
                            &format!("animation-delay:{}ms", delay_ms),
                        )?;
                    }
                }

                if x == self.cursor_x && y == self.cursor_y {
                    classes.push("active");
                }

                button.set_class_name(&classes.join(" "));
                button.set_text_content(Some(&label));

                let _ = self.board.append_child(&button)?;
            }
        }

        Ok(())
    }

    fn sync_custom_visibility(&self) -> Result<(), JsValue> {
        if self.difficulty_select.value() == "custom" {
            self.custom_settings.set_class_name("custom-settings");
        } else {
            self.custom_settings
                .set_class_name("custom-settings custom-settings-hidden");
        }

        Ok(())
    }

    fn choice_from_controls(&self) -> Result<DifficultyChoice, String> {
        match self.difficulty_select.value().as_str() {
            "beginner" => Ok(preset_choice(DifficultyPreset::Beginner)),
            "intermediate" => Ok(preset_choice(DifficultyPreset::Intermediate)),
            "expert" => Ok(preset_choice(DifficultyPreset::Expert)),
            "custom" => {
                let width = parse_input_usize(&self.custom_width, "Width")?;
                let height = parse_input_usize(&self.custom_height, "Height")?;
                let mines = parse_input_usize(&self.custom_mines, "Mines")?;

                let settings = validate_custom(width, height, mines)?;
                Ok(DifficultyChoice {
                    best_key: format!("custom-{width}x{height}-{mines}"),
                    storage_value: format!("custom:{width}:{height}:{mines}"),
                    settings,
                })
            }
            _ => Err("Unsupported difficulty option".to_string()),
        }
    }
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

fn by_id<T: JsCast>(document: &Document, id: &str) -> Result<T, JsValue> {
    document
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str(&format!("Missing element with id '{id}'")))?
        .dyn_into::<T>()
        .map_err(|_| JsValue::from_str(&format!("Element '{id}' had unexpected type")))
}

fn event_coords(event: &Event) -> Option<(usize, usize)> {
    let target = event.target()?;
    let element = target.dyn_into::<Element>().ok()?;

    let x = element.get_attribute("data-x")?.parse::<usize>().ok()?;
    let y = element.get_attribute("data-y")?.parse::<usize>().ok()?;
    Some((x, y))
}

fn should_ignore_key_event(event: &KeyboardEvent) -> bool {
    let Some(target) = event.target() else {
        return false;
    };
    let Ok(element) = target.dyn_into::<Element>() else {
        return false;
    };

    matches!(element.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA")
}

fn parse_input_usize(input: &HtmlInputElement, label: &str) -> Result<usize, String> {
    input
        .value()
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("{label} must be a whole number."))
}

fn preset_choice(preset: DifficultyPreset) -> DifficultyChoice {
    match preset {
        DifficultyPreset::Beginner => DifficultyChoice {
            settings: preset.settings(),
            best_key: "beginner".to_string(),
            storage_value: "beginner".to_string(),
        },
        DifficultyPreset::Intermediate => DifficultyChoice {
            settings: preset.settings(),
            best_key: "intermediate".to_string(),
            storage_value: "intermediate".to_string(),
        },
        DifficultyPreset::Expert => DifficultyChoice {
            settings: preset.settings(),
            best_key: "expert".to_string(),
            storage_value: "expert".to_string(),
        },
    }
}

fn parse_saved_choice(raw: Option<&str>) -> Option<DifficultyChoice> {
    let value = raw?;
    match value {
        "beginner" => Some(preset_choice(DifficultyPreset::Beginner)),
        "intermediate" => Some(preset_choice(DifficultyPreset::Intermediate)),
        "expert" => Some(preset_choice(DifficultyPreset::Expert)),
        _ => {
            let mut parts = value.split(':');
            if parts.next()? != "custom" {
                return None;
            }

            let width = parts.next()?.parse::<usize>().ok()?;
            let height = parts.next()?.parse::<usize>().ok()?;
            let mines = parts.next()?.parse::<usize>().ok()?;
            if parts.next().is_some() {
                return None;
            }

            let settings = validate_custom(width, height, mines).ok()?;
            Some(DifficultyChoice {
                settings,
                best_key: format!("custom-{width}x{height}-{mines}"),
                storage_value: value.to_string(),
            })
        }
    }
}

fn apply_choice_to_controls(
    difficulty_select: &HtmlSelectElement,
    custom_width: &HtmlInputElement,
    custom_height: &HtmlInputElement,
    custom_mines: &HtmlInputElement,
    choice: &DifficultyChoice,
) {
    match choice.storage_value.as_str() {
        "beginner" | "intermediate" | "expert" => {
            difficulty_select.set_value(&choice.storage_value);
            custom_width.set_value("");
            custom_height.set_value("");
            custom_mines.set_value("");
        }
        _ => {
            difficulty_select.set_value("custom");
            custom_width.set_value(&choice.settings.width.to_string());
            custom_height.set_value(&choice.settings.height.to_string());
            custom_mines.set_value(&choice.settings.mines.to_string());
        }
    }
}

fn number_class(adjacent: u8) -> &'static str {
    match adjacent {
        1 => "n1",
        2 => "n2",
        3 => "n3",
        4 => "n4",
        5 => "n5",
        6 => "n6",
        7 => "n7",
        _ => "n8",
    }
}
