//! The commands a chat line may run, each one whole in its own entry.

use bota_proto::{ItemId, SlotId, Team, item_called, item_name};

use crate::game::Cheat;
use crate::run::Whom;

/// One command a chat line may run.
pub struct CommandDef {
    /// What it is typed as, after the slash.
    pub name: &'static str,
    /// Its arguments, as `/help` shows them. Empty for one that takes none.
    pub usage: &'static str,
    /// What it does, in a line.
    pub about: &'static str,
    /// Reads the words typed after the name into what is to be done, or
    /// says what is wrong with them.
    pub read: fn(&[&str]) -> Result<Deed, String>,
}

/// What a command comes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Deed {
    /// Lines for the one who typed it alone.
    Reply(Vec<String>),
    /// A number drawn between two, both included, for everybody present to
    /// see.
    Roll {
        /// The smallest it may be.
        low: u32,
        /// The largest it may be.
        high: u32,
    },
    /// A cheat taken for seats of the running match.
    Cheat {
        /// Which one.
        cheat: Cheat,
        /// For whom.
        whom: Whom,
    },
}

/// Every command, in the order `/help` lists them.
pub const COMMANDS: [CommandDef; 6] = [
    CommandDef {
        name: "help",
        usage: "[command]",
        about: "lists the commands, or tells how one is used",
        read: help,
    },
    CommandDef {
        name: "roll",
        usage: "[max] | <min> <max>",
        about: "draws a number, 1 to 100 unless told otherwise, for everybody to see",
        read: roll,
    },
    CommandDef {
        name: "gold",
        usage: "[amount] [@whom]",
        about: "cheat: gold for nothing, 1000 unless told otherwise",
        read: gold,
    },
    CommandDef {
        name: "lvlup",
        usage: "[levels] [@whom]",
        about: "cheat: the hero goes up a level, or as many as told",
        read: lvlup,
    },
    CommandDef {
        name: "refresh",
        usage: "[@whom]",
        about: "cheat: full health and mana, every cooldown cleared",
        read: refresh,
    },
    CommandDef {
        name: "item",
        usage: "<name or id> [@whom]",
        about: "cheat: an item for nothing, named as the shop names it",
        read: item,
    },
];

/// Reads a line typed after a slash into what it asks for.
pub fn read_command(line: &str) -> Result<Deed, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let Some((name, rest)) = words.split_first() else {
        return Err("a command goes after the slash; /help lists them".to_string());
    };
    let Some(command) = command_named(name) else {
        return Err(format!("no command /{name}; /help lists them"));
    };
    (command.read)(rest)
}

/// The command typed as this, case aside.
fn command_named(name: &str) -> Option<&'static CommandDef> {
    COMMANDS
        .iter()
        .find(|command| command.name.eq_ignore_ascii_case(name))
}

/// How a command is typed.
fn usage_of(command: &CommandDef) -> String {
    if command.usage.is_empty() {
        format!("/{}", command.name)
    } else {
        format!("/{} {}", command.name, command.usage)
    }
}

/// What to say when a command is typed wrong.
fn wrong(name: &str) -> String {
    match command_named(name) {
        Some(command) => format!("usage: {}", usage_of(command)),
        None => format!("usage: /{name}"),
    }
}

fn help(words: &[&str]) -> Result<Deed, String> {
    match words {
        [] => Ok(Deed::Reply(
            COMMANDS
                .iter()
                .map(|command| format!("{} - {}", usage_of(command), command.about))
                .collect(),
        )),
        [name] => {
            let name = name.trim_start_matches('/');
            let command = command_named(name)
                .ok_or_else(|| format!("no command /{name}; /help lists them"))?;
            Ok(Deed::Reply(vec![format!(
                "{} - {}",
                usage_of(command),
                command.about
            )]))
        }
        _ => Err(wrong("help")),
    }
}

fn roll(words: &[&str]) -> Result<Deed, String> {
    let (low, high) = match words {
        [] => (1, 100),
        [high] => (1, number(high)?),
        [low, high] => (number(low)?, number(high)?),
        _ => return Err(wrong("roll")),
    };
    if low > high {
        return Err(format!("nothing lies between {low} and {high}"));
    }
    Ok(Deed::Roll { low, high })
}

fn gold(words: &[&str]) -> Result<Deed, String> {
    let (words, whom) = split_whom(words)?;
    let amount = match words {
        [] => 1000,
        [amount] => number(amount)?,
        _ => return Err(wrong("gold")),
    };
    Ok(Deed::Cheat {
        cheat: Cheat::Gold { amount },
        whom,
    })
}

fn lvlup(words: &[&str]) -> Result<Deed, String> {
    let (words, whom) = split_whom(words)?;
    let count = match words {
        [] => 1,
        [count] => number(count)?,
        _ => return Err(wrong("lvlup")),
    };
    Ok(Deed::Cheat {
        cheat: Cheat::Levels { count },
        whom,
    })
}

fn refresh(words: &[&str]) -> Result<Deed, String> {
    let (words, whom) = split_whom(words)?;
    if !words.is_empty() {
        return Err(wrong("refresh"));
    }
    Ok(Deed::Cheat {
        cheat: Cheat::Refresh,
        whom,
    })
}

fn item(words: &[&str]) -> Result<Deed, String> {
    let (words, whom) = split_whom(words)?;
    if words.is_empty() {
        return Err(wrong("item"));
    }
    let named = words.join(" ");
    let item = match named.parse::<u16>() {
        Ok(id) => Some(ItemId(id)).filter(|id| item_name(*id).is_some()),
        Err(_) => item_called(&named),
    }
    .ok_or_else(|| format!("no item called '{named}'"))?;
    Ok(Deed::Cheat {
        cheat: Cheat::Item { item },
        whom,
    })
}

/// A number typed as an argument.
fn number<T: std::str::FromStr>(word: &str) -> Result<T, String> {
    word.parse()
        .map_err(|_| format!("'{word}' is not a number that fits here"))
}

/// Takes the seats a cheat is for off the end of its words: `@<seat>`,
/// `@radiant`, `@dire` or `@all`, and every seat of the one typing it when
/// none is named.
fn split_whom<'a, 'b>(words: &'a [&'b str]) -> Result<(&'a [&'b str], Whom), String> {
    let Some((last, rest)) = words.split_last() else {
        return Ok((words, Whom::Mine));
    };
    let Some(named) = last.strip_prefix('@') else {
        return Ok((words, Whom::Mine));
    };
    let whom = if named.eq_ignore_ascii_case("all") {
        Whom::All
    } else if named.eq_ignore_ascii_case("radiant") {
        Whom::Side(Team::Radiant)
    } else if named.eq_ignore_ascii_case("dire") {
        Whom::Side(Team::Dire)
    } else if let Ok(seat) = named.parse::<u8>() {
        Whom::Seat(SlotId(seat))
    } else {
        return Err(format!(
            "'@{named}' names nobody: @<seat>, @radiant, @dire or @all"
        ));
    };
    Ok((rest, whom))
}
