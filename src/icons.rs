//! Ícones Phosphor registrados como fallback da fonte proporcional.
//!
//! `egui-nerdfonts` está travado no egui 0.27 (forçaria downgrade); o
//! `egui-phosphor` 0.14 acompanha o egui 0.36 e resolve os mesmos casos
//! (setas da árvore, pasta, engrenagem, etc.).

/// Constantes de ícones (`&str` pronto para `format!`/`RichText`).
pub use egui_phosphor::variants::regular as P;

/// Monta "ícone + rótulo" para botões e menus.
#[must_use]
pub fn labeled(icon: &str, label: &str) -> String {
    if label.is_empty() {
        icon.to_owned()
    } else {
        format!("{icon} {label}")
    }
}
