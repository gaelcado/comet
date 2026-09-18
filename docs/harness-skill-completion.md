# Skill completion across harnesses

Settings → Keyboard Shortcuts has two independent controls for each production
harness: Codex, Claude Code, Cursor, OpenCode, Devin, Grok, Hermes, and Pi.

- **Use $ for skills** enables the skill picker after `$`.
- **Separate skills from / commands** removes skills from the `/` picker.

Codex defaults to both enabled. Other harnesses default to both disabled, keeping
skills available through `/`. To use Claude Code or OpenCode with `$` for skills
and `/` for commands, enable both controls for that harness. Enabling `$` alone
makes skills available in both pickers. The preferences are saved per harness;
existing “Show skills in / menu” preferences are preserved until overridden.

These controls change completion, not arbitrary typed text. Selecting a skill
stores its identity in the composer. Literal dollar signs, code, and unselected
text retain their meaning.

## Discovery and delivery

Discovery runs on the selected host in the project directory. Codex uses its
native skill catalog. Other adapters discover standard project and user skill
directories, including shared `.agents/skills` directories. Claude checks native
command availability; OpenCode adds skills from its server command catalog.
ACP adapters bind discovered skills to advertised commands, and Pi also exposes
advertised `/skill:name` entries without a local file.

At delivery, Codex receives typed skill inputs. A leading skill selection with a
native command for the current harness becomes that command, preserving its
arguments. Other selections include an explicit skill reference in the prompt.
Native command metadata is scoped to its harness, so switching agents does not
invoke another agent’s same-named command. Only catalog entries identified as
skills are removed from the slash picker when separation is enabled.

File discovery covers the standard directories implemented by each adapter;
custom plugin paths and provider configuration are exposed where the native
catalog provides them. It is not a complete parser of every provider’s config.

Provider references:

- [Claude Code skills](https://code.claude.com/docs/en/skills)
- [OpenCode skills](https://opencode.ai/v2/docs/skills)
- [OpenCode command source and skill classification](https://github.com/anomalyco/opencode/blob/dev/packages/opencode/src/command/index.ts)
- [Cursor skills](https://prod.cursor.com/docs/skills)
- [Grok skills](https://docs.x.ai/build/features/skills-plugins-marketplaces)
- [Hermes skills](https://hermes-agent.nousresearch.com/docs/user-guide/features/skills)
- [Pi skills](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/skills.md)
