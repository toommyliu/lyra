pub(crate) mod auto_zone;

use super::bridge::{self, CallError, Loader};

pub(crate) fn walk_to(loader: &impl Loader, x: i32, y: i32) -> Result<bool, CallError> {
    if bridge::player::get_hp(loader)? <= 0 {
        return Ok(false);
    }
    bridge::player::walk_to(loader, x, y, None)
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use crate::automation::{bridge::Value, testing::FakeLoader};

    fn player(hp: i32) -> (FakeLoader, Rc<RefCell<Vec<(f64, f64)>>>) {
        let walks = Rc::new(RefCell::new(Vec::new()));
        let seen = walks.clone();
        let loader = FakeLoader::new(move |callback, args| {
            Ok(match (callback, args) {
                ("player.getHp", _) => Value::Number(f64::from(hp)),
                ("player.walkTo", [Value::Number(x), Value::Number(y), _]) => {
                    seen.borrow_mut().push((*x, *y));
                    Value::Bool(true)
                }
                (other, _) => panic!("unexpected call {other} {args:?}"),
            })
        });
        (loader, walks)
    }

    #[test]
    fn a_dead_player_does_not_walk() {
        let (dead, walks) = player(0);
        assert_eq!(walk_to(&dead, 10, 20), Ok(false));
        assert!(walks.borrow().is_empty());
        let (alive, walks) = player(500);
        assert_eq!(walk_to(&alive, 10, 20), Ok(true));
        assert_eq!(*walks.borrow(), [(10.0, 20.0)]);
    }
}
