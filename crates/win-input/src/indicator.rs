//! Session visibility (006d): while a remote session is live, the
//! person at the computer should be able to tell. On Windows this
//! sets the console title to a persistent warning and raises a
//! toast notification; the agent's console banner is printed
//! regardless of platform.

/// Mark the start of a remote session controlled by `controller`.
pub fn session_started(controller: &str) {
    #[cfg(windows)]
    imp::started(controller);
    #[cfg(not(windows))]
    let _ = controller;
}

/// Clear the indicator when the session ends.
pub fn session_ended() {
    #[cfg(windows)]
    imp::ended();
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::System::Console::{GetConsoleTitleW, SetConsoleTitleW};

    static SAVED_TITLE: Mutex<Option<String>> = Mutex::new(None);
    const ACTIVE_TITLE: &str = "⚠ ABHIVERSE — REMOTE SESSION ACTIVE";

    fn set_title(title: &str) {
        let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let _ = SetConsoleTitleW(PCWSTR(wide.as_ptr()));
        }
    }

    fn current_title() -> String {
        let mut buf = vec![0u16; 512];
        let n = unsafe { GetConsoleTitleW(&mut buf) } as usize;
        String::from_utf16_lossy(&buf[..n.min(buf.len())])
    }

    pub fn started(controller: &str) {
        if let Ok(mut saved) = SAVED_TITLE.lock() {
            if saved.is_none() {
                *saved = Some(current_title());
            }
        }
        set_title(ACTIVE_TITLE);
        toast(
            "Abhiverse remote session started",
            &format!("'{controller}' is viewing and may control this computer."),
        );
    }

    pub fn ended() {
        let restore = SAVED_TITLE
            .lock()
            .ok()
            .and_then(|mut s| s.take())
            .unwrap_or_else(|| "Abhiverse Agent".to_string());
        set_title(&restore);
        toast(
            "Abhiverse remote session ended",
            "Remote access has stopped.",
        );
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    /// Best-effort toast: it needs a registered app identity on the
    /// machine, so failures are ignored — the title and the console
    /// banner are the guaranteed indicators.
    fn toast(title: &str, body: &str) {
        let xml = format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
            xml_escape(title),
            xml_escape(body)
        );
        let _ = (|| -> windows::core::Result<()> {
            let doc = windows::Data::Xml::Dom::XmlDocument::new()?;
            doc.LoadXml(&HSTRING::from(xml))?;
            let notification =
                windows::UI::Notifications::ToastNotification::CreateToastNotification(&doc)?;
            let notifier =
                windows::UI::Notifications::ToastNotificationManager::CreateToastNotifierWithId(
                    &HSTRING::from("Abhiverse"),
                )?;
            notifier.Show(&notification)?;
            Ok(())
        })();
    }
}
