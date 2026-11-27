//! Sorts apps into the launcher's categories, as the Start menu's
//! "Category" view does, from their names and ids (Windows keeps the
//! Store's categories to itself).

use norbanner_core::fuzzy::fold;

use crate::{icons, strings::text};

/// In the order the launcher shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Social,
    Productivity,
    Creativity,
    Entertainment,
    Games,
    Development,
    Utilities,
    Other,
}

pub const ALL: [Category; 8] = [
    Category::Social,
    Category::Productivity,
    Category::Creativity,
    Category::Entertainment,
    Category::Games,
    Category::Development,
    Category::Utilities,
    Category::Other,
];

impl Category {
    pub fn label(self) -> &'static str {
        let s = text();
        match self {
            Self::Social => s.cat_social,
            Self::Productivity => s.cat_productivity,
            Self::Creativity => s.cat_creativity,
            Self::Entertainment => s.cat_entertainment,
            Self::Games => s.cat_games,
            Self::Development => s.cat_development,
            Self::Utilities => s.cat_utilities,
            Self::Other => s.cat_other,
        }
    }

    /// A glyph for when the category is shown on its own.
    pub fn glyph(self) -> char {
        match self {
            Self::Social => icons::CHAT,
            Self::Productivity => icons::WORK,
            Self::Creativity => icons::BRUSH,
            Self::Entertainment => icons::MOVIE,
            Self::Games => icons::SPORTS_ESPORTS,
            Self::Development => icons::TERMINAL,
            Self::Utilities => icons::HANDYMAN,
            Self::Other => icons::APPS,
        }
    }
}

/// Words (in an app's folded name or lowercase id) that place it, checked
/// in this order: the first match wins.
const RULES: [(Category, &[&str]); 7] = [
    (
        Category::Games,
        &[
            "steam",
            "epic games",
            "xbox",
            "minecraft",
            "roblox",
            "fortnite",
            "riot",
            "valorant",
            "league of legends",
            "battle.net",
            "ubisoft",
            "gog galaxy",
            "ea app",
            "origin",
            "rockstar",
            "game",
            "juego",
            "jeu",
            "jogo",
            "playnite",
            "heroic",
            "itch",
            "wow.exe",
            "warcraft",
            "hoyoplay",
            "genshin",
            "osu",
        ],
    ),
    (
        Category::Entertainment,
        &[
            "spotify",
            "youtube",
            "netflix",
            "prime video",
            "disney",
            "twitch",
            "vlc",
            "media player",
            "reproductor",
            "music",
            "musica",
            "musique",
            "movies",
            "peliculas",
            "films",
            "tv",
            "crunchyroll",
            "plex",
            "kodi",
            "itunes",
            "deezer",
            "tidal",
            "podcast",
        ],
    ),
    (
        Category::Social,
        &[
            "whatsapp",
            "discord",
            "telegram",
            "teams",
            "zoom",
            "slack",
            "skype",
            "outlook",
            "mail",
            "correo",
            "linkedin",
            "messenger",
            "instagram",
            "facebook",
            "signal",
            "x.com",
            "twitter",
            "thunderbird",
            "edge",
            "chrome",
            "brave",
            "firefox",
            "opera",
            "vivaldi",
            "browser",
            "navegador",
            "tor ",
        ],
    ),
    (
        Category::Development,
        &[
            "visual studio",
            "vscode",
            "code",
            "terminal",
            "powershell",
            "git",
            "github",
            "python",
            "node",
            "docker",
            "android studio",
            "jetbrains",
            "intellij",
            "pycharm",
            "webstorm",
            "rider",
            "cursor",
            "antigravity",
            "windsurf",
            "wsl",
            "ubuntu",
            "debian",
            "postman",
            "sql",
            "claude",
            "chatgpt",
            "copilot",
            "gemini",
            "ollama",
            "lm studio",
            "unity",
            "godot",
            "unreal",
            "devtoys",
            "symbol",
            "simbolo",
            "command prompt",
        ],
    ),
    (
        Category::Creativity,
        &[
            "photos",
            "fotos",
            "paint",
            "camera",
            "camara",
            "photoshop",
            "illustrator",
            "figma",
            "blender",
            "gimp",
            "krita",
            "inkscape",
            "obs",
            "clipchamp",
            "capcut",
            "davinci",
            "premiere",
            "after effects",
            "lightroom",
            "canva",
            "audacity",
            "affinity",
            "designer",
            "screensketch",
            "snipping",
            "recortes",
        ],
    ),
    (
        Category::Productivity,
        &[
            "word",
            "excel",
            "powerpoint",
            "onenote",
            "office",
            "notion",
            "obsidian",
            "acrobat",
            "pdf",
            "explorer",
            "explorador",
            "notepad",
            "bloc de notas",
            "to do",
            "todo",
            "calendar",
            "calendario",
            "evernote",
            "libreoffice",
            "writer",
            "calc ",
            "drive",
            "dropbox",
            "onedrive",
            "trello",
            "files",
        ],
    ),
    (
        Category::Utilities,
        &[
            "calculator",
            "calculadora",
            "clock",
            "reloj",
            "alarm",
            "settings",
            "configuracion",
            "parametres",
            "control panel",
            "panel de control",
            "task manager",
            "administrador",
            "administracion",
            "7-zip",
            "winrar",
            "cleanup",
            "liberador",
            "remote desktop",
            "escritorio remoto",
            "conexion a escritorio",
            "narrator",
            "narrador",
            "magnifier",
            "lupa",
            "voice access",
            "acceso por voz",
            "on-screen keyboard",
            "teclado en pantalla",
            "windows tools",
            "herramientas",
            "system",
            "sistema",
            "immersivecontrolpanel",
            "windowscalculator",
            "windowsalarms",
            "quick assist",
            "asistencia",
            "get help",
            "tips",
            "recovery",
            "defragment",
            "registry",
            "registro",
            "powertoys",
            "everything",
            "nvidia",
            "amd software",
            "intel",
            "driver",
            "bluetooth",
            "print",
            "impres",
            "scan",
            "escan",
            "backup",
            "security",
            "seguridad",
            "vpn",
            "launcher",
            "installer",
            "update",
        ],
    ),
];

/// The category of the app named `name` with id `id`.
#[must_use]
pub fn classify(name: &str, id: &str) -> Category {
    let name = format!(" {} ", fold(name));
    let id = id.to_ascii_lowercase();
    for (category, words) in RULES {
        if words.iter().any(|word| {
            contains_word(&name, word) || (word.trim().len() >= 5 && id.contains(word.trim()))
        }) {
            return category;
        }
    }
    Category::Other
}

/// `word` in `name` at a word start ("tv" is not in "atv"); short words
/// must also end there ("obs" is not "obsidian").
fn contains_word(name: &str, word: &str) -> bool {
    let word = word.trim();
    let bytes = name.as_bytes();
    name.match_indices(word).any(|(i, _)| {
        let starts = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
        let end = i + word.len();
        let ends = word.len() > 4 || end >= bytes.len() || !bytes[end].is_ascii_alphanumeric();
        starts && ends
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_apps_land_where_expected() {
        assert_eq!(classify("Epic Games Launcher", ""), Category::Games);
        assert_eq!(classify("XBOX", ""), Category::Games);
        assert_eq!(classify("Spotify", ""), Category::Entertainment);
        assert_eq!(classify("YouTube Music", ""), Category::Entertainment);
        assert_eq!(classify("WhatsApp", ""), Category::Social);
        assert_eq!(classify("Brave", ""), Category::Social);
        assert_eq!(classify("Terminal", ""), Category::Development);
        assert_eq!(classify("GitHub Desktop", ""), Category::Development);
        assert_eq!(classify("Paint", ""), Category::Creativity);
        assert_eq!(
            classify("Explorador de archivos", ""),
            Category::Productivity
        );
        assert_eq!(classify("Calculadora", ""), Category::Utilities);
        assert_eq!(classify("Configuración", ""), Category::Utilities);
        assert_eq!(classify("Some Thing", ""), Category::Other);
    }

    #[test]
    fn ids_help_and_words_must_start() {
        assert_eq!(
            classify(
                "Kalkulator",
                "shell:AppsFolder\\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"
            ),
            Category::Utilities
        );
        // "tv" inside a word is not TV.
        assert_eq!(classify("Gravity", ""), Category::Other);
        assert_eq!(classify("Obsidian", ""), Category::Productivity);
    }
}
