//! Selected official Dioxus Components, copied from revision 9a758255ea26e2b20c8cecf4c1c946feb9e71da7.
//! Source: https://github.com/DioxusLabs/components; MIT license is colocated.
//! Components are source templates by design; keep this provenance when updating them.
// Preserve the official component's variant API when selecting only some variants here.
#[allow(dead_code)]
#[path = "button/component.rs"]
mod button;
#[path = "card/component.rs"]
mod card;
pub(crate) use button::{Button, ButtonVariant};
pub(crate) use card::{Card, CardContent, CardDescription, CardHeader, CardTitle};
