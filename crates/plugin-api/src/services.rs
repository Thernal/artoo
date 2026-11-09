//! What plugins offer each other (#175): a contract a plugin publishes — its own crate of traits and plain types,
//! as the app's features do — offered under a key, `name@major` (`jira.api@1`), and asked for by the key. The
//! plugin that asks names the key in its `Meta::requires`, so it is installed after the one offering it.
//!
//! The value is any `'static` type: an `Rc<dyn Api>`, an entity. Both plugins build the contract's crate against
//! the same SDK, so its types are the same types in both, and the value asked for is the value offered.

use gpui::{App, Global};
use std::any::{Any, TypeId};
use std::collections::BTreeMap;

#[derive(Default)]
struct Services(BTreeMap<String, Box<dyn Any>>);

impl Global for Services {}

/// Offers `service` under `key` (`name@major`), in place of what was offered under it before.
pub fn offer<T: 'static>(cx: &mut App, key: &str, service: T) {
    (cx.default_global::<Services>().0).insert(key.to_owned(), Box::new(service));
}

/// The service under `key` as a `T`, or why not: none offered, or one of another type — which is what a contract
/// built twice, its two builds' types differing, looks like.
pub fn service<T: Clone + 'static>(cx: &App, key: &str) -> Result<T, String> {
    let offered = (cx.try_global::<Services>())
        .and_then(|s| s.0.get(key))
        .ok_or_else(|| format!("nothing is offered as {key}"))?;
    offered.downcast_ref::<T>().cloned().ok_or_else(|| {
        format!(
            "{key} is offered, but not as a {} (asked {:?}, offered {:?})",
            std::any::type_name::<T>(),
            TypeId::of::<T>(),
            (**offered).type_id(),
        )
    })
}

/// The keys offered, in order.
pub fn offered(cx: &App) -> Vec<String> {
    (cx.try_global::<Services>())
        .map(|s| s.0.keys().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    trait Greet {
        fn greet(&self, name: &str) -> String;
    }

    struct Hello;

    impl Greet for Hello {
        fn greet(&self, name: &str) -> String {
            format!("hello, {name}")
        }
    }

    #[test]
    fn a_service_is_asked_for_by_its_key_and_type() {
        gpui::TestAppContext::single().update(|cx| {
            offer(cx, "hello.api@1", Rc::new(Hello) as Rc<dyn Greet>);
            let api = service::<Rc<dyn Greet>>(cx, "hello.api@1").unwrap();
            assert_eq!(api.greet("ana"), "hello, ana");
            assert_eq!(offered(cx), ["hello.api@1"]);
            let other = service::<Rc<String>>(cx, "hello.api@1").unwrap_err();
            assert!(
                other.starts_with("hello.api@1 is offered, but not as a "),
                "{other}"
            );
            let none = service::<Rc<dyn Greet>>(cx, "hello.api@2").err();
            assert_eq!(none.as_deref(), Some("nothing is offered as hello.api@2"));
        });
    }
}
