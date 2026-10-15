# Domain context

## Theme vocabulary

- **Theme family** — A named collection of related theme variants that share an origin, such as Night Owl and Night Owl Light.
- **Theme variant** — One complete, resolved palette for a single appearance (`light` or `dark`). Runtime UI consumes variants, not source-format tokens.
- **Theme source** — The durable origin of a custom family: an imported snapshot, linked file, linked package, or editable native file.
- **Imported snapshot** — A self-contained copy of a compiled theme family. It no longer follows changes to its original source.
- **Linked theme** — A custom family that follows a source on disk and can be reloaded without re-importing it.
- **Linked file** — A link to one VS Code-compatible theme definition.
- **Linked package** — A link to a VS Code extension folder or `package.json` that declares one or more related theme variants.
- **Editable theme** — A duplicate stored as a native resolved-family JSON file. Users edit the file directly and explicitly reload it; invalid edits preserve the last known good family.
- **Last known good** — The most recent successfully compiled family retained by a linked theme when its current source is missing or invalid.
- **Import report** — A per-variant summary of mapped roles, fallbacks, unsupported values, and inferred decisions produced during compilation.
- **Mapping review** — The optional advanced view of an import report. Normal theme selection and import do not expose token names.
- **Theme hardening** — Deterministic post-mapping repairs that prefer stronger semantically related source colors, minimally adjust only shared Zeron roles when needed, and record every decision in the mapping review.
- **Theme default accent** — The interaction accent chosen or inferred for a theme variant by its authoring source.
- **Accent override** — A Zeron preset that replaces interaction roles only; syntax, terminal ANSI, diff, warning, error, and success colors remain owned by the theme.
- **Recommended surface treatment** — A theme variant's authored recommendation for whether its surfaces are frosted or opaque. It is used only when the user keeps the surface preference at theme default.
- **Surface preference** — A device-local choice of theme default, frosted, or opaque that is independent of appearance, theme, and accent selections.
- **Resolved surface treatment** — The effective frosted or opaque treatment produced by applying the surface preference to the active variant's recommendation.

## Project widget vocabulary

- **Visual** — Interactive content an agent shows inline in a chat. It belongs to that chat until someone saves it.
- **Project widget** — A visual kept in one project space, on the device that owns the space, independent of the chat it came from. Not the Settings UI primitives also called widgets.
- **Owner** — The device whose engine holds a space's widgets. Other devices reach them only through it; there is no replica.
- **Bundle** — One immutable version of a widget's content: its script and assets. New content is a new bundle, never an edit.
- **Refresh recipe** — The instruction and agent choice a refresh runs. It never holds a credential.
- **Refresh attempt** — One agent run asked to rebuild one widget. It changes the widget only if its turn completes successfully.
- **Candidate** — Content a refresh attempt proposed, validated but not yet published.
- **Last success** — When the widget's current content was validated. Distinct from the last attempt and from the age of the data it shows.

## Zeron MCP vocabulary

- **Zeron MCP** — The MCP server Zeron gives its own agents. Distinct from third-party MCP servers an agent may also use.
- **Bridge** — One running Zeron MCP server, attached to the engine it joined.
- **Tool policy** — The switches and description overrides of the Zeron MCP tools, owned by one engine and profile.
- **Description override** — Replacement text for a tool's description. It changes what agents read, never what the tool may do.
- **Admission** — The engine's decision, at the moment of each call, whether a tool may run.
