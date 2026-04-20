use rusqlite::{params, OptionalExtension};

use ozone_core::session::UnixTimestamp;

use crate::Result;

use super::{generate_uuid_like, SqliteRepository};

/// A character card stored in the global library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredCharacter {
    pub card_id: String,
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub personality: String,
    pub scenario: String,
    pub greeting: String,
    pub example_dialogue: String,
    pub created_at: UnixTimestamp,
    pub updated_at: UnixTimestamp,
}

impl SqliteRepository {
    pub fn create_character(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
        system_prompt: impl Into<String>,
    ) -> Result<StoredCharacter> {
        let card_id = generate_uuid_like();
        let now = self.now();
        let name = name.into();
        let description = description.into();
        let system_prompt = system_prompt.into();

        let conn = self.ensure_global_connection()?;
        conn.execute(
            "INSERT INTO character_cards (card_id, name, description, system_prompt, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![card_id, name, description, system_prompt, now, now],
        )?;

        Ok(StoredCharacter {
            card_id,
            name,
            description,
            system_prompt,
            personality: String::new(),
            scenario: String::new(),
            greeting: String::new(),
            example_dialogue: String::new(),
            created_at: now,
            updated_at: now,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_character_full(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
        system_prompt: impl Into<String>,
        personality: impl Into<String>,
        scenario: impl Into<String>,
        greeting: impl Into<String>,
        example_dialogue: impl Into<String>,
    ) -> Result<StoredCharacter> {
        let card_id = generate_uuid_like();
        let now = self.now();
        let name = name.into();
        let description = description.into();
        let system_prompt = system_prompt.into();
        let personality = personality.into();
        let scenario = scenario.into();
        let greeting = greeting.into();
        let example_dialogue = example_dialogue.into();

        let conn = self.ensure_global_connection()?;
        conn.execute(
            "INSERT INTO character_cards (card_id, name, description, system_prompt, personality, scenario, greeting, example_dialogue, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![card_id, name, description, system_prompt, personality, scenario, greeting, example_dialogue, now, now],
        )?;

        Ok(StoredCharacter {
            card_id,
            name,
            description,
            system_prompt,
            personality,
            scenario,
            greeting,
            example_dialogue,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn list_characters_global(&self) -> Result<Vec<StoredCharacter>> {
        let conn = self.ensure_global_connection()?;
        let mut stmt = conn.prepare(
            "SELECT card_id, name, description, system_prompt, personality, scenario, greeting, example_dialogue, created_at, updated_at
             FROM character_cards
             ORDER BY updated_at DESC",
        )?;

        let rows = stmt
            .query_map([], |row| {
                Ok(StoredCharacter {
                    card_id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    system_prompt: row.get(3)?,
                    personality: row.get(4)?,
                    scenario: row.get(5)?,
                    greeting: row.get(6)?,
                    example_dialogue: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn get_character(&self, card_id: &str) -> Result<Option<StoredCharacter>> {
        let conn = self.ensure_global_connection()?;
        let result = conn
            .query_row(
                "SELECT card_id, name, description, system_prompt, personality, scenario, greeting, example_dialogue, created_at, updated_at
                 FROM character_cards WHERE card_id = ?1",
                params![card_id],
                |row| {
                    Ok(StoredCharacter {
                        card_id: row.get(0)?,
                        name: row.get(1)?,
                        description: row.get(2)?,
                        system_prompt: row.get(3)?,
                        personality: row.get(4)?,
                        scenario: row.get(5)?,
                        greeting: row.get(6)?,
                        example_dialogue: row.get(7)?,
                        created_at: row.get(8)?,
                        updated_at: row.get(9)?,
                    })
                },
            )
            .optional()?;

        Ok(result)
    }

    pub fn delete_character(&self, card_id: &str) -> Result<bool> {
        let conn = self.ensure_global_connection()?;
        let affected = conn.execute(
            "DELETE FROM character_cards WHERE card_id = ?1",
            params![card_id],
        )?;
        Ok(affected > 0)
    }

    /// Update an existing character card.  All fields are overwritten.
    #[allow(clippy::too_many_arguments)]
    pub fn update_character(
        &self,
        card_id: &str,
        name: impl Into<String>,
        description: impl Into<String>,
        system_prompt: impl Into<String>,
        personality: impl Into<String>,
        scenario: impl Into<String>,
        greeting: impl Into<String>,
        example_dialogue: impl Into<String>,
    ) -> Result<StoredCharacter> {
        let now = self.now();
        let name = name.into();
        let description = description.into();
        let system_prompt = system_prompt.into();
        let personality = personality.into();
        let scenario = scenario.into();
        let greeting = greeting.into();
        let example_dialogue = example_dialogue.into();

        let conn = self.ensure_global_connection()?;
        let affected = conn.execute(
            "UPDATE character_cards
                SET name = ?2, description = ?3, system_prompt = ?4,
                    personality = ?5, scenario = ?6, greeting = ?7,
                    example_dialogue = ?8, updated_at = ?9
              WHERE card_id = ?1",
            params![card_id, name, description, system_prompt, personality, scenario, greeting, example_dialogue, now],
        )?;

        if affected == 0 {
            return Err(crate::PersistError::InvalidData(format!(
                "character card {card_id} not found"
            )));
        }

        // Retrieve the original created_at.
        let created_at: UnixTimestamp = conn.query_row(
            "SELECT created_at FROM character_cards WHERE card_id = ?1",
            params![card_id],
            |row| row.get(0),
        )?;

        Ok(StoredCharacter {
            card_id: card_id.to_owned(),
            name,
            description,
            system_prompt,
            personality,
            scenario,
            greeting,
            example_dialogue,
            created_at,
            updated_at: now,
        })
    }
}
