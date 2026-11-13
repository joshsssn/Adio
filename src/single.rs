//! Instance unique : un 2ᵉ lancement transmet son fichier à la fenêtre déjà ouverte, puis se ferme.
//!
//! Le canal est un tube nommé local, propre à l'utilisateur. Sa création échoue si une instance
//! existe déjà (premier tube exclusif), ce qui tranche aussi les lancements simultanés.

use interprocess::local_socket::{prelude::*, GenericNamespaced, ListenerOptions, Name};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;

pub enum Startup {
    /// Une instance tournait déjà : le fichier lui a été transmis, il faut quitter.
    Forwarded,
    /// Nous sommes l'instance principale (écoute éventuellement indisponible).
    Primary(Option<LocalSocketListener>),
}

fn name() -> std::io::Result<Name<'static>> {
    let user = std::env::var("USERNAME").unwrap_or_default();
    format!("adio-player-{user}.sock").to_ns_name::<GenericNamespaced>()
}

pub fn startup(arg: Option<&Path>) -> Startup {
    let Ok(n) = name() else { return Startup::Primary(None) };
    match ListenerOptions::new().name(n.clone()).create_sync() {
        Ok(listener) => Startup::Primary(Some(listener)),
        Err(_) => {
            // Une instance existe : on lui envoie le chemin (ligne vide = simple mise au premier plan).
            allow_foreground();
            for _ in 0..30 {
                if let Ok(mut s) = LocalSocketStream::connect(n.clone()) {
                    let line = arg.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                    if writeln!(s, "{line}").and_then(|_| s.flush()).is_ok() {
                        return Startup::Forwarded;
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            // L'autre instance ne répond pas : on démarre quand même plutôt que de ne rien afficher.
            Startup::Primary(None)
        }
    }
}

/// Écoute les lancements suivants et transmet leurs chemins à l'application.
pub fn listen(listener: LocalSocketListener, tx: Sender<Option<PathBuf>>, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        for conn in listener.incoming().filter_map(Result::ok) {
            let mut line = String::new();
            if BufReader::new(conn).read_line(&mut line).is_err() {
                continue;
            }
            let t = line.trim();
            let path = (!t.is_empty()).then(|| PathBuf::from(t));
            if tx.send(path).is_err() {
                return;
            }
            ctx.request_repaint();
        }
    });
}

/// Autorise l'instance principale à passer au premier plan (sinon Windows ne fait que clignoter).
fn allow_foreground() {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(
            windows_sys::Win32::UI::WindowsAndMessaging::ASFW_ANY,
        );
    }
}
