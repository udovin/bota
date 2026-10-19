//! What a command line asks for, read on its own.

use bota_proto::{ItemId, SlotId, Team};

use crate::commands::{Deed, read_command};
use crate::game::Cheat;
use crate::run::Whom;

#[test]
fn a_cheat_is_for_the_typist_unless_it_names_whom() {
    assert_eq!(
        read_command("gold"),
        Ok(Deed::Cheat {
            cheat: Cheat::Gold { amount: 1000 },
            whom: Whom::Mine,
        })
    );
    assert_eq!(
        read_command("GOLD -200 @dire"),
        Ok(Deed::Cheat {
            cheat: Cheat::Gold { amount: -200 },
            whom: Whom::Side(Team::Dire),
        }),
        "any case, a debt, a side"
    );
    assert_eq!(
        read_command("lvlup 3 @2"),
        Ok(Deed::Cheat {
            cheat: Cheat::Levels { count: 3 },
            whom: Whom::Seat(SlotId(2)),
        })
    );
    assert_eq!(
        read_command("refresh @all"),
        Ok(Deed::Cheat {
            cheat: Cheat::Refresh,
            whom: Whom::All,
        })
    );
    assert_eq!(
        read_command("refresh @nobody"),
        Err("'@nobody' names nobody: @<seat>, @radiant, @dire or @all".to_string())
    );
}

#[test]
fn an_item_goes_by_the_name_the_shop_shows_or_by_its_id() {
    let wand = Ok(Deed::Cheat {
        cheat: Cheat::Item {
            item: ItemId(crate::game::ITEM_MAGIC_WAND),
        },
        whom: Whom::Mine,
    });
    assert_eq!(read_command("item wand"), wand);
    assert_eq!(read_command("item 36"), wand);
    assert_eq!(
        read_command("item ogre axe @radiant"),
        Ok(Deed::Cheat {
            cheat: Cheat::Item {
                item: ItemId(crate::game::ITEM_OGRE_AXE),
            },
            whom: Whom::Side(Team::Radiant),
        }),
        "a name of two words, then whom"
    );
    assert_eq!(
        read_command("item 999"),
        Err("no item called '999'".to_string())
    );
    assert_eq!(
        read_command("item"),
        Err("usage: /item <name or id> [@whom]".to_string())
    );
}

#[test]
fn a_roll_and_the_numbers_it_is_given() {
    assert_eq!(read_command("roll"), Ok(Deed::Roll { low: 1, high: 100 }));
    assert_eq!(read_command("roll 20"), Ok(Deed::Roll { low: 1, high: 20 }));
    assert_eq!(
        read_command("roll 5 10"),
        Ok(Deed::Roll { low: 5, high: 10 })
    );
    assert_eq!(
        read_command("roll 10 5"),
        Err("nothing lies between 10 and 5".to_string())
    );
    assert_eq!(
        read_command("roll lots"),
        Err("'lots' is not a number that fits here".to_string())
    );
    assert_eq!(
        read_command("lvlup 300"),
        Err("'300' is not a number that fits here".to_string())
    );
    assert_eq!(
        read_command(""),
        Err("a command goes after the slash; /help lists them".to_string())
    );
}
