//! Значок в трее: клик — показать окно, меню «Открыть Murmur / Выйти».
//! Обработчики вызываются в UI-потоке из цикла сообщений Windows — даже когда окно спрятано и кадры не рисуются,
//! поэтому окно показываем прямо отсюда (`desktop::show`), а выход передаём через флаг.

#[cfg(windows)]
pub use imp::Tray;
#[cfg(not(windows))]
pub use stub::Tray;

#[cfg(windows)]
mod imp {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use eframe::egui;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    use crate::desktop::{self, Window};

    /// 32×32, та же картинка, что у exe (assets/icon.ico).
    const ICON_PNG: &[u8] = include_bytes!("../assets/icon-32.png");

    pub struct Tray {
        _icon: TrayIcon,
        quit: Arc<AtomicBool>,
    }

    impl Tray {
        /// Создать значок. Ошибка (нет панели задач и т.п.) — без трея, «закрыть» тогда просто закрывает.
        pub fn new(window: Window, ctx: egui::Context) -> Option<Self> {
            let rgba = image::load_from_memory(ICON_PNG).ok()?.into_rgba8();
            let (w, h) = rgba.dimensions();
            let icon = Icon::from_rgba(rgba.into_raw(), w, h).ok()?;
            let open = MenuItem::new("Открыть Murmur", true, None);
            let exit = MenuItem::new("Выйти", true, None);
            let menu = Menu::with_items(&[&open, &PredefinedMenuItem::separator(), &exit]).ok()?;
            let tray = TrayIconBuilder::new()
                .with_icon(icon)
                .with_tooltip("Murmur")
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .map_err(|e| log::warn!("трей недоступен: {e}"))
                .ok()?;
            let quit = Arc::new(AtomicBool::new(false));

            TrayIconEvent::set_event_handler(Some({
                let ctx = ctx.clone();
                move |e: TrayIconEvent| {
                    let left_up = matches!(
                        e,
                        TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                            | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
                    );
                    if left_up {
                        desktop::show(window);
                        ctx.request_repaint();
                    }
                }
            }));
            let (open_id, exit_id) = (open.id().clone(), exit.id().clone());
            MenuEvent::set_event_handler(Some({
                let quit = quit.clone();
                move |e: MenuEvent| {
                    if e.id == open_id {
                        desktop::show(window);
                    } else if e.id == exit_id {
                        quit.store(true, Ordering::SeqCst);
                    }
                    ctx.request_repaint();
                }
            }));
            Some(Self { _icon: tray, quit })
        }

        /// Отладка: как будто «Выйти» нажали через `after` (флаг ставится из другого потока).
        #[cfg(debug_assertions)]
        pub fn debug_quit_in(&self, after: std::time::Duration, ctx: egui::Context) {
            let quit = self.quit.clone();
            std::thread::spawn(move || {
                std::thread::sleep(after);
                log::info!("отладка: «Выйти» из трея");
                quit.store(true, Ordering::SeqCst);
                ctx.request_repaint();
            });
        }

        /// Выбрано «Выйти» в меню трея.
        pub fn quit_requested(&self) -> bool {
            self.quit.load(Ordering::SeqCst)
        }
    }
}

#[cfg(not(windows))]
mod stub {
    use crate::desktop::Window;
    use eframe::egui;

    /// Трея нет (не Windows): «закрыть» закрывает.
    pub struct Tray;

    impl Tray {
        pub fn new(_window: Window, _ctx: egui::Context) -> Option<Self> {
            None
        }

        pub fn quit_requested(&self) -> bool {
            false
        }

        #[cfg(debug_assertions)]
        pub fn debug_quit_in(&self, _after: std::time::Duration, _ctx: egui::Context) {}
    }
}
