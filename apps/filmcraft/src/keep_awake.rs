//! Playback-scoped desktop idle inhibition. All bus calls stay off the UI thread.

pub fn service() -> Option<Box<dyn FnMut(bool)>> {
    #[cfg(target_os = "linux")]
    {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::Builder::new().name("playback-idle-inhibit".into()).spawn(move || {
            if std::panic::catch_unwind(|| run(receive)).is_err() {
                log::error!("Playback idle inhibition worker failed");
            }
        });
        if let Err(error) = worker {
            log::warn!("Cannot start playback idle inhibition: {error}");
            return None;
        }
        let mut sent = false;
        Some(Box::new(move |playing| {
            if playing != sent && send.try_send(playing).is_ok() {
                sent = playing;
            }
        }))
    }
    #[cfg(not(target_os = "linux"))]
    None
}

#[cfg(target_os = "linux")]
struct Inhibitor {
    proxy: zbus::blocking::Proxy<'static>,
    cookie: u32,
    release: &'static str,
}

#[cfg(target_os = "linux")]
impl Drop for Inhibitor {
    fn drop(&mut self) {
        if let Err(error) = self.proxy.call::<_, _, ()>(self.release, &(self.cookie,)) {
            log::warn!("Could not release playback idle inhibition: {error}");
        }
        // The dedicated bus connection also releases the inhibitor on disconnect or app exit.
    }
}

#[cfg(target_os = "linux")]
fn acquire() -> zbus::Result<Inhibitor> {
    let conn = zbus::blocking::connection::Builder::session()?.method_timeout(std::time::Duration::from_secs(2)).build()?;
    let reason = "Playing a video";
    let gnome = zbus::blocking::Proxy::new(&conn, "org.gnome.SessionManager", "/org/gnome/SessionManager", "org.gnome.SessionManager")?;
    // GNOME's idle flag prevents automatic dimming/blanking without blocking manual suspend.
    if let Ok(cookie) = gnome.call("Inhibit", &("YouCraft", 0u32, reason, 8u32)) {
        return Ok(Inhibitor { proxy: gnome, cookie, release: "Uninhibit" });
    }
    let proxy = zbus::blocking::Proxy::new(&conn, "org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver", "org.freedesktop.ScreenSaver")?;
    let cookie = proxy.call("Inhibit", &("YouCraft", reason))?;
    Ok(Inhibitor { proxy, cookie, release: "UnInhibit" })
}

#[cfg(target_os = "linux")]
fn run(receive: std::sync::mpsc::Receiver<bool>) {
    let mut inhibitor = None;
    while let Ok(playing) = receive.recv() {
        if playing && inhibitor.is_none() {
            match acquire() {
                Ok(active) => inhibitor = Some(active),
                Err(error) => log::warn!("Desktop could not inhibit dimming during playback: {error}"),
            }
        } else if !playing {
            inhibitor = None;
        }
    }
}
