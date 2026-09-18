//! Durable composer references. Only canonical links created by completion are
//! decoded; ordinary slash/dollar text remains ordinary prompt text.
use serde::{Deserialize, Serialize};
use std::ops::Range;

pub const INVOCATION_SCHEME: &str = "zeron-invoke:";

/// Native catalog entries may have no local skill file (for example plugin-provided commands).
pub fn native_skill_identity(path: &str) -> bool {
    path.starts_with("opencode-skill:") || path.starts_with("harness-skill:")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCommand {
    pub name: String,
    pub harness: crate::HarnessId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub path: String,
    pub description: String,
    pub enabled: bool,
    /// A provider-advertised slash invocation for this skill, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<SkillCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Invocation {
    Command {
        name: String,
    },
    Skill {
        name: String,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        command: Option<SkillCommand>,
    },
}

impl Invocation {
    pub fn name(&self) -> &str {
        match self {
            Self::Command { name } | Self::Skill { name, .. } => name,
        }
    }
    pub fn prefix(&self) -> char {
        match self {
            Self::Command { .. } => '/',
            Self::Skill { .. } => '$',
        }
    }
    pub fn detail(&self) -> String {
        match self {
            Self::Command { name } => format!("/{name}"),
            Self::Skill { path, .. } => path.clone(),
        }
    }
    pub fn link(&self) -> String {
        let json = serde_json::to_vec(self).expect("invocation serializes");
        let payload: String = json.iter().map(|b| format!("{b:02x}")).collect();
        let label = self
            .name()
            .replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]");
        format!("[{}{label}]({INVOCATION_SCHEME}{payload})", self.prefix())
    }
    /// Skills retain their selected identity as an ordinary Markdown file
    /// reference. Commands stay in place so provider prefix semantics survive.
    pub fn prompt_text(&self) -> String {
        match self {
            Self::Command { name } => format!("/{name}"),
            Self::Skill { name, path, .. } => {
                if native_skill_identity(path) {
                    return format!("{name}");
                }
                let path: String = path
                    .bytes()
                    .map(|b| {
                        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
                            (b as char).to_string()
                        } else {
                            format!("%{b:02X}")
                        }
                    })
                    .collect();
                format!("[${name}]({path})")
            }
        }
    }
}

pub fn invocation_links(text: &str) -> Vec<(Range<usize>, Invocation)> {
    if !text.contains(INVOCATION_SCHEME) {
        return Vec::new();
    }
    let mut links = Vec::new();
    for (event, range) in pulldown_cmark::Parser::new(text).into_offset_iter() {
        let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) = event else {
            continue;
        };
        let Some(hex) = dest_url.strip_prefix(INVOCATION_SCHEME) else {
            continue;
        };
        let (start, end) = (range.start, range.end);
        if hex.len() % 2 != 0 || !hex.is_ascii() {
            continue;
        }
        let bytes: Option<Vec<u8>> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
            .collect();
        let Some(invocation) = bytes.and_then(|b| serde_json::from_slice::<Invocation>(&b).ok())
        else {
            continue;
        };
        if invocation.name().is_empty()
            || invocation
                .name()
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
        {
            continue;
        }
        if let Invocation::Skill { path, .. } = &invocation {
            if path.is_empty() || path.chars().any(char::is_control) {
                continue;
            }
        }
        if let Invocation::Skill {
            command: Some(command),
            ..
        } = &invocation
        {
            if command.name.is_empty()
                || command
                    .name
                    .chars()
                    .any(|c| !(c.is_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')))
            {
                continue;
            }
        }
        if invocation.link() == text[start..end]
            && links
                .last()
                .is_none_or(|(r, _): &(Range<usize>, Invocation)| r.end <= start)
        {
            links.push((start..end, invocation));
        }
    }
    links
}

/// Keep selected skill identity intact until Codex builds native input blocks.
/// Other providers receive readable Markdown and their advertised command text.
pub fn harness_prompt(text: &str, harness: crate::HarnessId) -> String {
    let text = crate::file_mentions::file_mention_prompt(text);
    if harness == crate::HarnessId::Codex {
        return text;
    }
    let mut result = String::new();
    let mut at = 0;
    for (range, invocation) in invocation_links(&text) {
        result.push_str(&text[at..range.start]);
        match &invocation {
            Invocation::Skill {
                command: Some(command),
                ..
            } if command.harness == harness && text[..range.start].trim().is_empty() => {
                // The catalog supplies the provider's canonical command, regardless
                // of whether the user selected the skill through $ or /.
                result.push_str(&format!("/{}", command.name));
            }
            Invocation::Skill {
                path,
                command: Some(command),
                ..
            } if native_skill_identity(path) && command.harness == harness => {
                result.push_str(&format!("Use the skill /{}", command.name));
            }
            Invocation::Skill { .. } => {
                // Inline invocations and file-only providers need explicit intent;
                // a dollar token by itself has no universal provider semantics.
                result.push_str("Use the skill ");
                result.push_str(&invocation.prompt_text());
            }
            _ => result.push_str(&invocation.prompt_text()),
        }
        at = range.end;
    }
    result.push_str(&text[at..]);
    result
}

pub fn invocation_prompt(text: &str) -> String {
    let mut result = String::new();
    let mut at = 0;
    for (range, invocation) in invocation_links(text) {
        result.push_str(&text[at..range.start]);
        result.push_str(&invocation.prompt_text());
        at = range.end;
    }
    result.push_str(&text[at..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selected_native_skills_route_by_provider_and_keep_arguments() {
        for harness in [crate::HarnessId::ClaudeCode, crate::HarnessId::Opencode] {
            let skill = Invocation::Skill {
                name: "plugin:review".into(),
                path: "/repo/SKILL.md".into(),
                command: Some(SkillCommand {
                    name: "plugin:review".into(),
                    harness,
                }),
            };
            let raw = format!("{} inspect tests", skill.link());
            assert_eq!(
                harness_prompt(&raw, harness),
                "/plugin:review inspect tests"
            );
            assert!(harness_prompt(&raw, crate::HarnessId::Cursor).starts_with("Use the skill "));
            assert!(
                harness_prompt(&format!("Please {}", skill.link()), harness)
                    .contains("Use the skill ")
            );
            assert_eq!(invocation_links(&raw)[0].1, skill);
        }
        for harness in [
            crate::HarnessId::Cursor,
            crate::HarnessId::Devin,
            crate::HarnessId::Grok,
            crate::HarnessId::Hermes,
            crate::HarnessId::Pi,
        ] {
            let skill = Invocation::Skill {
                name: "review".into(),
                path: "/repo/SKILL.md".into(),
                command: None,
            };
            assert_eq!(
                harness_prompt(&skill.link(), harness),
                "Use the skill [$review](/repo/SKILL.md)"
            );
        }
    }

    #[test]
    fn delivery_preserves_native_skill_identity_only_for_codex() {
        let skill = Invocation::Skill {
            command: None,
            name: "review".into(),
            path: "/repo/with space/SKILL.md".into(),
        };
        let raw = format!(
            "Use {} on {}",
            skill.link(),
            crate::file_mentions::local_file_link("src/lib.rs", false)
        );
        let codex = harness_prompt(&raw, crate::HarnessId::Codex);
        assert_eq!(invocation_links(&codex)[0].1, skill);
        assert!(codex.ends_with("[lib.rs](src/lib.rs)"));
        for harness in [crate::HarnessId::ClaudeCode, crate::HarnessId::Opencode] {
            let prompt = harness_prompt(&raw, harness);
            assert_eq!(
                prompt,
                "Use Use the skill [$review](/repo/with%20space/SKILL.md) on [lib.rs](src/lib.rs)"
            );
        }
    }

    #[test]
    fn durable_references_round_trip_without_reordering() {
        let command = Invocation::Command {
            name: "compact".into(),
        };
        let skill = Invocation::Skill {
            command: None,
            name: "review".into(),
            path: "/repo/a b/SKILL.md".into(),
        };
        let raw = format!("first {} then {} finally", command.link(), skill.link());
        assert_eq!(invocation_links(&raw).len(), 2);
        assert_eq!(
            invocation_prompt(&raw),
            "first /compact then [$review](/repo/a%20b/SKILL.md) finally"
        );
        assert_eq!(invocation_prompt("/$not-a-chip"), "/$not-a-chip");
        assert!(invocation_links("[x](zeron-invoke:bad)").is_empty());
        for code in [
            format!("`{}`", command.link()),
            format!("```\n{}\n```", command.link()),
            format!("\\{}", command.link()),
        ] {
            assert!(invocation_links(&code).is_empty());
            assert_eq!(invocation_prompt(&code), code);
        }
    }
}
