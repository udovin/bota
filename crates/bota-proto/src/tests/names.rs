use crate::*;

#[test]
fn an_item_answers_to_its_name_whatever_the_case() {
    assert_eq!(item_called("Butterfly"), Some(ItemId(46)));
    assert_eq!(
        item_called("ogre axe"),
        Some(ItemId(16)),
        "two words, any case"
    );
    assert_eq!(item_called("Ogre"), None, "half a name is not one");
    assert_eq!(item_called(""), None);
}

#[test]
fn every_name_is_told_back_by_its_id_and_no_two_are_alike() {
    for (at, name) in ITEM_NAMES.iter().enumerate() {
        let id = ItemId(at as u16);
        assert_eq!(item_name(id), Some(*name));
        assert_eq!(item_called(name), Some(id), "{name} is nobody else's");
    }
    assert_eq!(item_name(ItemId(ITEM_NAMES.len() as u16)), None);
}
