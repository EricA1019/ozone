# Ozone+ Character Creation Plan
**Session:** Running Ozone+ & Creating a Character Using Existing Flow  
**Date:** 2026-04-25  
**Goal:** Explore and document the user journey for creating a character in ozone-plus TUI

---

## Phase 1: Launch ozone-plus

### Entry Point
```
$ ozone-plus handoff --launcher-session
```

### Expected Flow
1. **Main Menu** — session management hub with tabs (Sessions, Characters, Settings, Help)
2. **Characters Screen** — character card CRUD interface
3. **Character Create/Import Screen** — build a new character

---

## Phase 2: Character Creation Flow

### Step-by-Step Journey Map

| Step | Action | Expected UI | Data Model Impact |
|------|--------|-------------|-------------------|
| 1 | Launch ozone-plus | Main Menu with tabs (Sessions, Characters, Settings, Help) | — |
| 2 | Select **Characters** tab | Character list view (empty initially) | `CharacterCard` entries in `global.db` |
| 3 | Choose **Create New** | Form to input character name, description, traits, lore | New `CharacterCard` record created |
| 4 | Fill character fields | Textareas for Name, Description, Traits, First Message, Lore | Fields mapped to `CharacterCard` struct |
| 5 | Save character | Confirmation prompt or auto-save | Insert into `characters` table in `global.db` |
| 6 | Select the new character | Character selected in list | `prefs.last_character_name` updated |
| 7 | Start a session with this character | Session created with `character_id` linked | `SessionSummary.character_id` populated |

---

## Phase 3: Data Model Discovered

### Character Schema (`global.db`)
```sql
CREATE TABLE character_cards (
    card_id TEXT PRIMARY KEY,      -- UUID v4
    name TEXT NOT NULL,
    description TEXT,
    system_prompt TEXT,
    personality TEXT,
    scenario TEXT,
    greeting TEXT,
    example_dialogue TEXT,
    created_at TEXT,
    updated_at TEXT
);
```

### TUI Character Detail Struct
```rust
pub struct CharacterDetail {
    pub card_id: String,          // empty on create
    pub name: String,
    pub description: String,
    pub system_prompt: String,
    pub personality: String,
    pub scenario: String,
    pub greeting: String,
    pub example_dialogue: String,
}
```

### Complete Flow Summary

| Step | Action | UI State | Data Impact |
|------|--------|----------|-------------|
| 1 | Launch `ozone-plus handoff --launcher-session` | Main Menu (Sessions, Characters, Settings, Help) | — |
| 2 | Press `C` for **Characters** tab | CharacterManager screen | — |
| 3 | Press `n` for **New** | CharacterCreate form with 8 fields | — |
| 4 | Fill in character details (Name, Description, System Prompt, Personality, Scenario, Greeting, Example Dialogue) | Form populated | — |
| 5 | Press `Enter` / Save | Confirmation or auto-save | INSERT into `character_cards` |
| 6 | Character appears in list | Back to CharacterManager | New entry visible |
| 7 | (Optional) Start session with character | Session creation prompt | `SessionSummary.character_id` linked |

---

## Character Fields Explained

| Field | Purpose | Example |
|-------|---------|----------|
| **Name** | Display name of the character | "Lady Moonwhisper" |
| **Description** | Brief bio/overview | "A celestial archivist from the Astral Archives..." |
| **System Prompt** | Core identity & roleplay rules | "[System] You are Lady Moonwhisper, an ethereal archivist..." |
| **Personality** | Tone, quirks, speech patterns | "Melancholic, poetic, obsessed with forgotten stars" |
| **Scenario** | Current situation/context | "The Archives have been breached by a cosmic anomaly..." |
| **Greeting** | First message to user | "You stand in the breach. What shall we preserve, traveler?" |
| **Example Dialogue** | Few-shot examples of conversation style | User: "Who are you?"\nLady: "I am the Keeper of Lost Constellations..." |

---

## Key Takeaways

### Strengths
- Clean separation between TUI, persistence, and domain logic
- SQLite with FTS5 for efficient character search
- Character cards are first-class citizens with rich metadata
- Example dialogue field enables few-shot prompting out of the box

### Considerations
- 8 fields is a good balance — not overwhelming but comprehensive
- Missing: voice/tone presets, avatar/image, tags for filtering, version history
- System prompt and scenario might benefit from templating variables

---

## Next Steps
1. ✅ Plan documented
2. ⏳ Execute ozone-plus launch and character creation
3. ⏳ Capture terminal output for each step
4. ⏳ Verify end-to-end flow works as expected
5. ⏳ Document any deviations or missing features
