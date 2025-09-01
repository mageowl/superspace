use std::collections::HashMap;

use serde::{Deserialize, Deserializer, de};

use super::UserCommand;

pub(super) fn deserialize<'de, D: Deserializer<'de, Error = E>, E: de::Error>(
    deser: D,
) -> Result<HashMap<String, (UserCommand, usize)>, D::Error> {
    let vec: Vec<UserCommand> = Vec::deserialize(deser)?;
    let mut map = HashMap::new();

    for (i, cmd) in vec.into_iter().enumerate() {
        if map.contains_key(&cmd.prefix) {
            return Err(E::custom(format!("duplicate command {}", cmd.prefix)));
        }
        map.insert(cmd.prefix.clone(), (cmd, i));
    }

    Ok(map)
}
