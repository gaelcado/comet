# Widgets de projet et contrôle du MCP Zeron — plan d’implémentation

Statut : proposition détaillée, **aucune fonctionnalité implémentée**. Inspection du 15 octobre 2026.

## 1. Cadre de travail et résultat attendu

- Worktree unique : `/Users/gaelcado/zeron/work/project-widgets-mcp-settings`.
- Branche unique : `feat/project-widgets-mcp-settings`.
- HEAD et base `origin/main` locale inspectés : `6ac8859998909bc5afd55838afb0635c281f31c3`. Aucun commit propre à la branche au début de l’inspection ; arbre propre.
- Les deux aspects restent dans **cette même branche**, avec une série de commits ordonnée. La règle générique du skill de contribution « un comportement par branche » est remplacée ici par l’instruction explicite utilisateur. Aucun autre checkout ne doit recevoir d’écriture.
- Cette tâche produit uniquement ce document : pas de code de fonctionnalité, changement de branche, fetch/rebase, commit, push, PR, test Cargo, build ou lancement d’application.
- Le brief utilisateur est la référence produit. Il reprend la conversation analysée dans le chat `d75d1fe7-2a75-4661-8f03-940340a36d3e` ; ce chat n’a pas été relancé. Son ancien découpage multibranche est caduc.
- Contexte des agents reporté et exclu. Ne pas relancer Voice Glyph, la CI panels ou le diagnostic de contexte vocal.

But du premier périmètre : conserver un dashboard créé dans un chat, le retrouver et le gérer dans son projet, puis l’actualiser volontairement via un agent ; permettre d’inspecter et de régler les outils du **MCP intégré Zeron**, avec un refus effectif des outils désactivés.

## 2. État des lieux vérifié dans cette base

Les chemins ci-dessous sont relatifs à la racine. Les symboles sont les points d’entrée ; les numéros de ligne sont des repères dans le SHA ci-dessus, susceptibles de bouger pendant l’implémentation.

| Sujet | Code existant et point d’intégration | Conséquence pour le plan |
| --- | --- | --- |
| Runtime visuel | `crates/visual/src/lib.rs`, `Visual`, `check_with_assets`, `seed_for` ; `assets.rs`, `node.rs`, `prelude.luau` ; `crates/visual-gpu` | Réutiliser Luau et le rendu natif. Ne pas réintroduire un moteur HTML ni développer un moteur de widgets parallèle. Sandbox sans réseau/I/O/`require`, mémoire et temps bornés. |
| Bornes de publication | `crates/proto/src/agent.rs:418`, `VISUAL_MAX_*` | Script 256 Kio, titre 200 caractères, 16 assets, 8 Mio par asset et 10 Mio cumulés. Réutiliser les validations et la contrainte de trame RPC 16 Mio, sans gonfler les watchers. |
| Catalogue MCP | `crates/mcp/src/tools.rs:34`, `ToolDef`, `catalog()` ; `Tools::list/has/call` vers 660 | Catalogue statique, descriptions `&'static str`, schémas JSON ; dispatch central. Aujourd’hui seul `show_visual` est filtré hors chat. Pas de préférences par outil. |
| Protocole MCP | `crates/mcp/src/jsonrpc.rs`, `handle_request`, `serve_stdio` | `tools/list` synchrone, `tools/call` vérifie `has` puis appelle `call`. `listChanged:false`. Les notifications entrantes, dont l’annulation, sont ignorées. Il faut modifier la garde du dispatch, pas seulement la liste. |
| Connexion MCP → moteur | `crates/mcp/src/zeron.rs`, `Origin`, `Zeron`, `call`, `resolve_space`, `resolve_target` ; `crates/mcp/src/lib.rs`, `McpConfig::from_env` | Proxy IPC reconnectant. Résolution projet/appareil existante à conserver ; aucun owner de préférences dans le subprocess stdio. |
| Injection agents | `crates/engine/src/sessions.rs:1575`, `Inner::zeron_mcp`, injection avant dispatch vers 2220 ; `apps/zeron/src/main.rs`, sous-commande `mcp` | Serveur injecté avec port, chat et appareil d’origine. Les sessions persistantes peuvent conserver un catalogue ancien. Ne pas modifier les MCP tiers de l’utilisateur. |
| Exemple de catalogue mis en cache | `crates/harness/src/pi/mcp.mjs:70` | Pi liste une fois puis enregistre `zeron_<tool>`. Une nouvelle description n’apparaît donc pas nécessairement dans un runtime déjà ouvert. |
| Publication inline | `crates/mcp/src/tools.rs:1209`, `Tools::show_visual`, `read_visual_assets` ; `crates/engine/src/rpc.rs:3759`, `PUBLISH_VISUAL` | Le bridge lit les fichiers dans le cwd de l’agent, envoie les bytes ; le moteur vérifie, stocke et injecte `AgentEvent::Visual`. Il exige un run vivant sur cet appareil avant/après validation. Ce RPC n’est pas un CRUD de widgets et ne doit pas devenir implicitement tel. |
| Stockage visuel actuel | `crates/engine/src/uploads.rs:903`, `store_visual`, `store_visual_assets`, `remove_visual` | Script `{id}-visual.luau`, manifest `{id}-assets.json`, fichiers `{id}-asset-{name}`. Publication avec rollback local ; chemins absolus d’uploads dans le transcript. Pas de propriété durable de widget, révision ni index projet. |
| Lecture/rendu | `crates/ui/src/visuals.rs:175`, `VisualFrame::new`, `start`, `VisualLayoutChanged` ; `crates/ui/src/transcript.rs:6709`, `render_visual` ; `crates/ui/src/attachments.rs:634`, `read_attachment_bytes` | Réutiliser le renderer et le chargement borné. Le seed est actuellement déduit de l’ID de ligne/visuel ; un widget devra transmettre explicitement le seed de son contenu. Les tâches/images sont libérées au relâchement de l’entité. |
| Pièce de chat | `crates/proto/src/agent.rs`, `AgentEvent::Visual` ; `crates/doc/src/parts.rs`, `MessagePart::Visual` ; `crates/doc/src/schema.rs` | La pièce contient ID, chemin, titre ; ce n’est pas un widget enregistré. Garder ce format lisible par les anciens clients. |
| Identité projet | `crates/proto/src/entities.rs:112`, `Space` ; `crates/proto/src/view.rs:137`, `project_key`, `representative_space` ; `crates/engine/src/spaces.rs` | Un espace est `(deviceId, path)`, ID stable et owner immuable. Le regroupement sidebar par `repositoryId` réunit des espaces, sans leur donner un propriétaire d’exécution commun. |
| Précédent host-local | `crates/engine/src/project_actions.rs`, `ProjectActionsStore::open(profile_store_root)`, `project-actions.json` ; `crates/engine/src/rpc.rs`, `local_project_action_space` | Modèle existant de données projet privées au moteur, adressées par espace et accessibles via relay. Réutiliser la résolution/validation d’owner, pas les commandes shell des Actions. |
| Profils et assemblage | `crates/engine/src/profile.rs`, `EngineProfile` ; `crates/engine/src/lib.rs:114`, `EngineConfig`, puis `EngineCore::assemble_with_profile_locked` | `EngineConfig` n’est pas un fichier de préférences MCP. EngineCore assemble les owners. Stores/journaux/uploads sont isolés par profil ; credentials, identité appareil et préférences providers sont device-local. |
| Sync réelle | `crates/doc/src/registry.rs`, `RegistryDoc` ; `crates/engine/src/workspace_host.rs` ; `crates/sync/src/store.rs`, `DocsStore` ; `crates/engine/src/rpc.rs`, `forwardable`, `is_stream_method` | Registry et chats ont leurs protocoles de sync. Ajouter un fichier ou une table locale ne les synchronise pas. Pour la V1 proposée, relayer les opérations vers l’owner, sans nouvelle famille de documents CRDT. |
| Assets à distance | `crates/engine/src/rpc.rs:3739`, `READ_ATTACHMENT_CHUNK` ; `crates/ui/src/attachments.rs` ; `edge/src/index.ts:396` | Les visuels sont lus sur le moteur hôte, par chunks. L’ancien miroir `/attachments/...` accepte puis jette les PUT : ne pas promettre une réplication R2 des assets. Les sidecars d’outils R2 sont un autre mécanisme. |
| Import de profil | `crates/engine/src/local_import.rs`, `LocalImporter` ; assemblage uploads dans `crates/engine/src/lib.rs` | Un import local→synced existe déjà, avec des grants d’uploads. Un nouveau store widgets ne sera pas importé automatiquement. `ARCHITECTURE.md` est partiellement historique sur ce sujet : le code prime. |
| Suppressions | `crates/engine/src/rpc.rs:1325`, `DeleteChat` ; `:1259`, `DeleteSpace` ; `WorkspaceHost::delete_chat/delete_space` ; `RegistryDoc::delete_space:1187` | DeleteChat purge le doc ; DeleteSpace tombstone l’espace et ses chats. Aujourd’hui pas de relation widget à maintenir. Prévoir leur traitement même si la suppression d’espace arrive par sync. |
| Cycle agent | `crates/engine/src/sessions.rs`, `dispatch`, `subscribe`, `interrupt`, `stop_turn`, `terminate`, `recover_stale` ; `crates/proto/src/entities.rs:308`, `Session.last_completed_turn` ; `crates/engine/src/run_journal.rs` ; `crates/doc/src/commands.rs` | Réutiliser le moteur, le marqueur de succès existant et l’annulation. `TurnListener` ne signale que le **début** d’un tour `(chat_id,cwd)` et sert au diff. `JournaledEvent` contient seq/event, sans commandId. Il manque le rattachement fiable de la tentative au tour exécuté. |
| Queue et redémarrage | `crates/engine/src/doc_host.rs:3407`, `queue_command`, `queue_command_with_transfers` ; `crates/engine/src/lib.rs:257`, appel à `recover_stale` | La queue génère aujourd’hui commandId en interne et désarchive un chat lorsqu’on lui envoie un message. La récupération peut relancer un run abandonné. Ces comportements doivent être adaptés étroitement pour les refresh, pas supposés compatibles. |
| Navigation/Settings | `crates/ui/src/shell.rs:539`, `SettingsSection`, `:690`, `Route`, outlet nouveau chat vers 11120 ; `crates/ui/src/settings.rs`, `SettingsStore` ; `settings/harnesses.rs` et `settings/accounts.rs` | Pas de route Widgets ni de section MCP. `settings/widgets.rs` contient les primitives de réglages, pas les widgets de projet. Ajouter des vues dédiées ; SettingsStore ne doit pas écrire les préférences moteur. |
| Compatibilité | `crates/proto/src/workspace.rs`, `EngineInfo.capabilities`, `capabilities::current` ; `crates/rpc/src/lib.rs`, `methods`, `RpcError` | Ajouter des capabilities explicites et des erreurs stables ; pas de déduction depuis le numéro de version de l’application. |

### Chevauchement GitHub vérifié

La [PR #882 — Show GitHub activity on the new-chat canvas](https://github.com/zeronsh/zeron/pull/882) est **ouverte**, non fusionnée lors de l’inspection. GitHub redirige `zeronsh/comet` vers `zeronsh/zeron` pour cette PR. Son périmètre : panneau sous le composer vide, compte GitHub et métriques 30 jours/heatmap, cache `github-activity.json`, dernier succès conservé, réglage General et masquage si l’ensemble ne tient pas.

Ses fichiers incluent `crates/ui/src/github_activity.rs`, `crates/engine/src/github_activity.rs`, les contrats proto/RPC, `shell.rs` et Settings. Ces modules ne sont pas présents dans la base inspectée.

**Décision recommandée :** route projet dédiée pour les widgets ; aucun nouveau panneau obligatoire sous le composer. #882 reste indépendant et n’est ni repris, ni modifié, ni prérequis. Réutiliser éventuellement une convention d’état « dernier succès + erreur de refresh », pas son cache par compte `gh`. Avant implémentation sur une base ultérieure, refaire une lecture ciblée de `shell.rs` et de l’état de #882 ; ne pas greffer son code sur cette branche à l’aveugle.

### Chevauchement MCP vérifié

Contrairement à une première lecture, plusieurs PR **ouvertes** touchent le serveur MCP Zeron et les fichiers que ce plan modifie (état vérifié le 15 octobre 2026) :

| PR | Portée | Fichiers partagés avec ce plan | Conséquence |
| --- | --- | --- | --- |
| [#878 — Zeron MCP: async delegation with background turn-end events](https://github.com/zeronsh/zeron/pull/878) | `create_chat(s)`/`send_message(s)` asynchrones, veille de fin de tour moteur (`NotifyTurnEnd`), livraison d’événements au harness (`DeliverEvent`) | `sessions.rs`, `doc_host.rs`, `rpc.rs`, `workspace_host.rs`, `tools.rs`, `zeron.rs`, `jsonrpc.rs`, `proto`, `rpc` | Elle construit précisément le seam « fin de tour corrélée » dont §7 constate l’absence. Avant l’étape 7, relire son état : si elle est fusionnée, réutiliser sa veille de fin de tour plutôt qu’ajouter un second mécanisme ; sinon décider explicitement d’attendre ou de concevoir un seam compatible. |
| [#898 — Visual replies: agents show HTML pages inline](https://github.com/zeronsh/zeron/pull/898) | Ancienne variante HTML de `show_visual`/`PublishVisual`, antérieure à la version Luau fusionnée (#900) | `visuals.rs`, `transcript.rs`, `uploads.rs`, `rpc.rs`, `tools.rs`, `agent.rs`, `parts.rs`, `docs/mcp.md` | Ne pas la reprendre : ce plan s’appuie sur le runtime Luau natif de HEAD. Vérifier qu’elle est fermée ou rebasée avant de considérer `show_visual` stable. |
| [#904 — MCP: add_project](https://github.com/zeronsh/zeron/pull/904) | Nouvel outil `add_project` et RPC `AddProject` non forwardable | `tools.rs`, `zeron.rs`, `jsonrpc.rs`, `rpc.rs`, `proto` | Conflit textuel certain avec l’extraction du catalogue (étape 1). Garder cette extraction comme un pur déplacement pour faciliter les rebases ; son outil entre dans le catalogue canonique dès fusion. |
| [#640 — MCP: search_chats](https://github.com/zeronsh/zeron/pull/640) | Nouvel outil `search_chats` et module `search.rs` | `tools.rs`, `lib.rs`, `docs/mcp.md` | Même remarque : conflit sur `tools.rs` et `docs/mcp.md`. |

Aucune PR ouverte ne porte sur des widgets persistants ni sur un réglage par outil du MCP Zeron. Les quatre PR ci-dessus ne sont ni reprises ni prérequises ; elles imposent de refaire une lecture ciblée de `crates/mcp/src/tools.rs`, `sessions.rs` et `doc_host.rs` avant les étapes 1, 2 et 7.

## 3. Décisions structurantes proposées

### 3.1 Portée et synchronisation

V1 : un widget appartient à **un espace projet existant, sur son moteur hôte, dans le profil actif**. Clé d’ownership `(profil, ownerDeviceId, spaceId)`. Son ID propre ne change ni au renommage du widget ni au renommage de l’espace. Le chemin du dossier n’est pas sa clé.

Le regroupement sidebar « projet » peut contenir plusieurs espaces du même dépôt. La page indique l’espace et l’appareil sélectionnés ; si plusieurs sont possibles, elle propose le même choix explicite que les parcours de projet actuels. Aucun merge silencieux entre clones/worktrees, aucune élection d’owner à partir du representative_space. Dans un groupe, une entrée Widgets ouvre d’abord l’espace courant, sinon un choix d’espace, sans forcer la création d’un chat.

| Situation | Comportement V1 recommandé |
| --- | --- |
| Profil Local | Persistance sous `profiles/local`, aucun transport cloud, aucun accès depuis un autre compte. |
| Profil Synced/Development | Store sous le root compte existant, données widgets toujours host-local. Les pairs autorisés peuvent consulter/modifier via le relay du moteur owner en ligne. |
| Owner hors ligne | Garder les résultats déjà chargés dans la vue et indiquer l’indisponibilité. Sur un nouveau client sans contenu chargé : indisponible, pas « aucun widget ». Pas de mutations/refresh mis en attente hors ligne dans cette V1. |
| Arrêt/redémarrage moteur | Réouverture du store et du dernier contenu validé ; les refresh interrompus deviennent `interrupted`, sans reprise automatique d’un appel payant. |
| Changement de profil | Les widgets et les overrides MCP du profil précédent ne sont pas visibles. Pas de migration automatique au login/logout. |

Il s’agit d’un accès multiappareil à l’owner, **pas d’une réplication hors ligne des widgets**. Ce compromis suit les Project Actions et évite d’inventer un consensus CAS sur un registry LWW. Une future réplication demanderait index durable, blobs adressés par hash, ownership et conflits documentés ; elle n’est pas obtenue par l’ajout d’un champ au registry. Si le produit exige une consultation depuis un autre appareil owner éteint dès la V1, cette extension change le périmètre et doit être décidée avant le chantier stockage.

Les réglages MCP appartiennent également au moteur joint et au profil actif, sans sync V1. Recommandation : `mcp-tool-settings.json` dans `EngineProfile::store_root()`, plutôt que `ui-settings.json`, un fichier projet versionné, ou le data_dir global. Les overrides de description peuvent contenir des consignes propres au compte ; l’isolation de profil est préférable à l’héritage global. Le sélecteur d’appareil Settings cible l’owner par RPC. Les deux stores utilisent le même profil capturé au boot et ne changent pas au milieu d’un runtime.

### 3.2 Interface commune, sans refonte transversale

- Un module moteur **ProjectWidgets** possède ensemble métadonnées, bundles durables, CAS, déduplication et refresh. Tous les appelants UI et MCP traversent son interface ; les invariants ne sont pas réimplémentés dans les vues.
- Un module moteur **McpToolPolicy** possède persistance, révision de politique et admission d’appels. Il ne possède ni comptes tiers ni configuration réseau de serveurs externes.
- Extraire les définitions statiques actuelles de `tools.rs` vers `crates/mcp/src/catalog.rs`, exportées par la crate. Le moteur et le bridge consomment cette seule source ; séparer des définitions possédées/sérialisables pour la description effective. Le dispatch conserve ses handlers existants.
- Recommandation minimale de graphe : déplacer la dépendance `zeron-mcp` du moteur de dev-dependencies vers dependencies pour consommer **uniquement** ce catalogue pur. La crate MCP dépend actuellement de proto/doc/rpc, pas du moteur : pas de cycle observé. Vérifier le graphe complet au chantier. Ne pas faire de retour IPC du moteur vers son propre serveur pour l’inspection. Une crate de catalogue séparée ne se justifierait que si cette dépendance crée un cycle réel ou alourdit une cible effectivement affectée.
- `proto` reçoit les contrats possédés et erreurs/capabilities, `rpc` les méthodes ; aucun type moteur ne doit fuiter vers l’UI. Le rendu natif demeure dans `ui::visuals`.

## 4. Parcours utilisateur

### 4.1 Sauvegarder et retrouver un visuel

Exemple : « Affiche les événements PostHog et les sessions Google Analytics des 30 derniers jours ». L’agent utilise les intégrations dont son harness dispose, produit le script et les assets, puis `show_visual` affiche le résultat dans le chat comme aujourd’hui.

Une action **Enregistrer dans le projet** est placée dans le chrome Zeron du visuel, hors de l’arbre Luau. Elle propose un titre et le projet du chat ; pour un chat sans projet ou dans un contexte ambigu, choix explicite d’un espace. Première version : sauvegarde dans le même moteur que la source ; un autre appareil implique une future copie explicite, pas un simple changement d’ID.

La sauvegarde ne lance pas l’agent et ne demande pas d’intégration. Le moteur relit la pièce source et copie son contenu dans un bundle autonome ; aucun chemin brut fourni par la vue ne suffit. Après succès : **Enregistré · Ouvrir les widgets**. Recliquer/rejouer une requête ne crée pas de doublon. Une éventuelle copie supplémentaire est une action explicite, avec un nouveau titre/ID.

Accès : menu du projet/sidebar → **Widgets**, disponible même sans chat sélectionné. La route possède un espace sélectionné et son propre état de navigation ; elle ne réutilise pas le faux chat vide comme identifiant. Aucun déplacement automatique du composer ni de la zone #882.

La page affiche les titres, les visuels, **Actualiser**, date du dernier succès, menu **Renommer / Déplacer avant-après / Supprimer**, et **Ouvrir le chat source**. Le drag reorder peut compléter les actions clavier, pas les remplacer. La suppression retire le widget ; elle ne supprime pas le chat. Un lien source absent affiche « Chat source supprimé » et le visuel reste utilisable. Pour un chat archivé encore présent, le lien l’ouvre sans le désarchiver implicitement.

Ne pas exposer à l’utilisateur les révisions, chemins, manifests, leases ou hashes. Une concurrence se présente comme « Ce widget a changé ailleurs. Rechargez-le avant de réessayer » avec l’état actuel ; pas de remplacement forcé silencieux.

### 4.2 Actualiser les métriques

Le premier refresh demande seulement ce qui manque : instruction d’actualisation, agent/modèle et appareil d’exécution déjà fixé par le projet. Préremplir une recette proposée lors de la création agent, mais rendre visible ce que l’utilisateur va relancer. Ne pas inférer une recette en rejouant tout le chat historique. Exemple : « Recalcule ces métriques PostHog et GA pour les 30 derniers jours, garde les définitions et les comparaisons ».

L’action indique qu’une exécution agent est lancée. Pas de réseau dans Luau, aucun credential dans le widget. Réutiliser la configuration et les permissions du harness sur l’owner ; une intégration PostHog/GA absente mène à un état d’aide ou à la demande d’authentification normale de cet agent. Ne pas promettre de découvrir globalement tous les serveurs tiers dans cette V1.

Pendant le refresh, le dernier résultat réussi reste visible, avec « Actualisation en cours », **Annuler** et **Ouvrir l’exécution**. Une demande d’entrée de l’agent devient « Réponse nécessaire » avec accès au chat de refresh. Une erreur affiche une phrase exploitable et **Réessayer** ; la date du dernier succès ne bouge pas. L’annulation affiche « Actualisation annulée », conserve le résultat, et permet une nouvelle tentative.

### 4.3 Placement et états

- Page projet : liste verticale de sections titrées, visuel en largeur disponible, cohérente avec le rendu inline existant ; pas de grille libre, ni redimensionnement/docking de cartes pour la V1. Le titre et les actions appartiennent à la page, le contenu au renderer.
- Vide confirmé : « Aucun widget dans ce projet » et action ouvrant un nouveau chat dans cet espace avec une demande de dashboard. Ne pas lancer automatiquement un agent.
- Chargement initial : skeleton léger dans le flux si l’attente dépasse environ 200 ms ; ne pas montrer le vide pendant la requête.
- Asset manquant ou corrompu : erreur locale à ce widget, autres widgets visibles ; relecture/réessai et accès à la source. Ne pas lancer un refresh agent pour réparer une simple lecture sans clic explicite.
- Owner hors ligne : état distinct de l’échec d’intégration et du vide ; désactiver les mutations et conserver ce qui est déjà affiché.
- Échec rendu/runtime : conserver le bundle validé ; proposer recharger, éviter boucle de relance automatique.
- Petite fenêtre : titres et actions reviennent à la ligne, menu accessible sans hover, contenu défile ; ne pas cacher les widgets persistants parce qu’ils ne tiennent pas sous un composer. Les grandes tables gardent leurs comportements natifs existants.
- Focus stable par widgetId, retour du focus après menu/dialogue/suppression, reorder clavier, libellés explicites des boutons icônes et annonces des statuts sans dépendre de la couleur. Respect des tokens thème, taille de texte et reduced-motion ; animations et GPU suspendus pour les visuels non visibles.

### 4.4 Inspecter et régler le MCP intégré

Settings → **MCP** → **Zeron**, avec sélecteur d’appareil et indication du profil actif. Recherche par nom/description ; une ligne par outil, switch et disponibilité. Distinguer « activé », « désactivé », « disponible uniquement depuis un chat », « moteur indisponible » ; Settings liste aussi les outils désactivés.

Déplier un outil montre description effective éditable, **Voir l’original**, **Réinitialiser la description**, et schéma d’entrée en lecture seule. Présenter aussi le schéma de sortie/annotations si le catalogue en fournit, sans en inventer. Édition avec **Enregistrer/Annuler**, validation inline, pas d’autosave par frappe. Réinitialiser retire l’override et fait suivre la description originale des futures versions. Le switch ne modifie pas le brouillon de description.

Une note concise explique : « Les nouveaux appels utilisent ce réglage. Certains agents doivent reconnecter le MCP pour actualiser leur liste d’outils. » Elle ne doit jamais laisser entendre qu’un outil désactivé reste exécutable avec un vieux catalogue. Pas de redémarrage automatique d’un agent occupé ; proposer la reconnexion au prochain point sûr lorsque nécessaire. La page reste opérable si tous les outils sont désactivés : les RPC de Settings ne sont pas eux-mêmes des outils MCP.

## 5. Modèle et persistance des widgets

### 5.1 Données proposées

Créer `crates/proto/src/project_widgets.rs` et `crates/engine/src/project_widgets.rs` (sous-modules privés stockage/refresh si cela simplifie la lecture).

| Donnée | Contrat proposé |
| --- | --- |
| Widget | `id` opaque UUID, `spaceId`, `ownerDeviceId`, `title`, `revision:u64`, `createdAt`, `updatedAt`, `lastSuccessfulAt?`, `contentRevision?`, `source?`, `refreshRecipe?`. Pas de secret. |
| Provenance | `sourceChatId`, identifiant de pièce visuelle, identifiant d’entrée/message s’il est disponible, titre source figé pour l’affichage et `sourceDeviceId`. Liens optionnels et non propriétaires. Le serveur résout la pièce par ID, pas par un chemin accepté aveuglément. |
| Contenu immuable | `bundleId`, `formatVersion`, `rendererFormat:luau-v1`, `renderSeed`, script et assets nommés avec taille/hash. `bundleId` nouveau à chaque contenu publié. Le seed de sauvegarde est celui du visuel source pour préserver les mêmes données aléatoires. |
| Recette | texte d’instruction, harness, modèle/reasoning optionnels, paramètres autorisés déjà supportés. Le cwd est résolu depuis l’espace owner, pas un cwd arbitraire persisté. Aucun token ni copie automatique du transcript. |
| Collection | `spaceId`, `collectionRevision`, liste ordonnée d’IDs. Ordre autoritaire au store, pas un index dépendant du rendu. |
| Refresh | `refreshId`, `widgetId`, `baseRevision`, `attemptRevision`, `commandId`, `executionChatId`, corrélation de tour, état, horodatages, `candidateBundleId?`, erreur bornée et expurgée. Une seule tentative active par widget. |
| Rejeu | `operationId`/`requestId` et empreinte du payload, réponse durable ; unicité de sauvegarde `(spaceId, sourceChatId, sourceVisualId)` et de `creationKey` agent. |

`revision` représente une modification durable du widget (titre/recette/contenu/suppression), pas les ticks de progression. Les transitions de refresh ont leur propre version ; le reorder change seulement `collectionRevision`. Cette séparation évite qu’un statut « running » fasse échouer un renommage. Toute modification du widget pendant une tentative rend son `baseRevision` obsolète : ne pas écraser, finir en conflit et inviter à relancer. On pourra fusionner finement titre/contenu plus tard si nécessaire.

### 5.2 Store et assets

Recommandation : SQLite moteur dédié `project-widgets.sqlite3` sous `profile.store_root()`, avec migrations transactionnelles versionnées. Le moteur dépend déjà de rusqlite. Ne pas étendre la responsabilité de `DocsStore` aux jobs/widgets host-local ni transformer un blob JSON en document CRDT sans protocole.

Tables logiques : widgets (avec tombstone), ordre/collection, tentatives, reçus d’idempotence ; contraintes uniques et CAS transactionnels. Les appels SQLite, copie/hash/check visuel sont hors des threads GPUI et hors d’un verrou async tenu pendant I/O. Un seul owner moteur grâce à InstanceLock ; transactions restent nécessaires pour plusieurs fenêtres/appels concurrents.

Bundles sous `profile.store_root()/project-widgets/assets/<bundleId>/`, manifest fermé, script et assets indépendants des uploads du chat. Écriture dans un répertoire temporaire adjacent, validation des noms/tailles/hash et `check_with_assets`, publication immuable par rename, puis transaction qui pointe le widget sur le bundle complet. Si la transaction échoue, le bundle devient orphelin collectable ; la référence précédente demeure. Ne jamais remplacer les fichiers d’un bundle lu par une vue.

Pour enregistrer une pièce : lire la source dans les uploads autorisés du profil, y compris les fallbacks legacy/import déjà autorisés, puis **copier** script et tous les assets dans le nouveau bundle. Ne pas garder un symlink, un chemin source, ni un hardlink susceptible d’être modifié. Un manifest qui nomme un fichier manquant fait échouer toute la sauvegarde. Revalider le bundle copié avec le seed d’origine.

Ajouter un RPC de lecture de bundle dédié, borné/chunké, résolu par `(widgetId, contentRevision, assetName)` et owner. Ce RPC ouvre seulement le root widgets du profil actif, rejette traversal/symlinks et permet de garder la jail des uploads actuelle inchangée. Réutiliser les conventions de chunks de `ReadAttachmentChunk`, pas élargir sa liste de roots aux chemins d’un client. Le script ne voyage pas dans `WatchProjectWidgets`.

Le renderer reçoit une source de contenu et un seed explicites : adapter de lecture uploads actuel pour le transcript, adapter de lecture bundle pour les widgets. Réutiliser `VisualFrame`/ses éléments, sans dupliquer ses milliers de lignes ; vérifier le comportement du cache par `(widgetId, contentRevision)` et la libération des images/tâches à changement de révision/route.

### 5.3 Concurrence, déduplication et suppression

1. Create exige un `operationId` stable ; pour l’agent, `creationKey` stable dans l’espace est également obligatoire. Rejouer la même clé/payload rend le même ID/révision. Une même clé avec un autre payload rend un conflit, jamais un update implicite.
2. Sauvegarder deux fois la même pièce dans le même espace retourne le widget existant, même si les deux fenêtres ont généré des operationId différents. Si ce widget a été supprimé, un nouveau clic explicite peut le recréer avec un nouvel ID et une nouvelle clé ; un ancien retry ne le ressuscite pas.
3. Update/Delete exigent `expectedRevision` ; mutation atomique `WHERE revision = expected AND deleted = false`, puis incrément. Un conflit retourne un code stable et les métadonnées courantes, jamais un dernier-écrivain-gagne silencieux. Les nouvelles créations ne se dédupliquent pas par titre.
4. Reorder exige la permutation exacte des IDs vivants et `expectedCollectionRevision` ; refuse doublon/ID étranger/liste partielle. Un ajout/suppression concurrent invalide cette permutation ; recharger avant retry.
5. Admission de refresh atomique : unicité d’une tentative active, `requestId` de clic/retry ; retourner la tentative active plutôt qu’en créer une seconde. Idempotence contrôle le lancement, pas seulement le remplacement final.
6. Delete pose un tombstone, invalide le refreshId actif et retire l’ID de la collection dans une transaction. L’annulation agent suit ; un résultat tardif ne peut plus publier. Les reçus empêchent de recréer via un retry ancien. Garder les tombstones/reçus de création tant que le store existe dans cette V1, plutôt qu’une expiration silencieuse qui compromettrait le contrat de rejeu.
7. Nettoyage : bundles non référencés par contenu courant ou tentative active, avec délai de grâce pour lectures en vol. Au début, GC conservateur, jamais suppression des uploads/transcripts depuis le store widgets. Un disque plein laisse le dernier bundle et le pointeur intacts.
8. Suppression d’un chat source ou de refresh : aucun cascade vers les widgets. La source devient un lien absent ; une tentative dont le chat d’exécution disparaît finit interrompue. Suppression d’espace : recommandé supprimer ses widgets et annuler leurs tentatives, avec information dans la confirmation existante. Réconciliation côté owner au démarrage et après changements du registry, car le tombstone d’espace peut venir d’un pair ; pas uniquement un hook du bouton local. Aucun déplacement implicite vers un autre clone.

## 6. Contrats RPC et MCP proposés

Les noms sont proposés pour l’implémentation ; ne pas ajouter des méthodes vides dans cette tâche. Sérialisation camelCase, IDs opaques, tailles bornées et erreurs typées compatibles avec `RpcError`/les erreurs capability. Étendre `crates/rpc/src/lib.rs::methods`, les DTO proto et `EngineRpc` ; ajouter les méthodes distantes à `forwardable` et les watchers à `is_stream_method`.

### 6.1 Interface widgets

| RPC proposé | Entrées significatives | Sortie / invariants |
| --- | --- | --- |
| `WatchProjectWidgets` | `spaceId`, `targetDeviceId?` | Snapshot initial puis snapshots bornés de métadonnées, ordre, collectionRevision et état refresh. Les suppressions sont observables. |
| `GetProjectWidget` | `widgetId`, `targetDeviceId?` | Métadonnées, recette lisible, révisions et manifest ; aucune récupération de transcript obligatoire. |
| `ReadProjectWidgetAssetChunk` | `widgetId`, `contentRevision`, `assetName` réservé ou nommé, offset, target | Bytes bornés, nextOffset/done et intégrité ; contenu immuable, erreur si absent/version retirée. |
| `SaveProjectVisual` | `spaceId`, `sourceChatId`, `sourceVisualId`, titre optionnel, `operationId`, target | Résolution de pièce fiable, copie bundle, create dédupliqué. Aucun run vivant requis ; pas de chemin arbitraire. |
| `CreateProjectWidget` | `spaceId`, `creationKey`, `operationId`, titre, contenu complet, recette optionnelle, target | Validation puis création idempotente ; contenu Luau et assets base64 comme PublishVisual. |
| `UpdateProjectWidget` | `widgetId`, `expectedRevision`, `operationId`, patch metadata ou nouveau contenu complet, `refreshId?`, target | Hors refresh : CAS et commit. Dans refresh : candidat validé et staged, **pas encore publié** ; aucune modification arbitraire d’un autre widget. |
| `ReorderProjectWidgets` | `spaceId`, IDs ordonnés, `expectedCollectionRevision`, operationId, target | Nouvelle collection autoritaire. |
| `DeleteProjectWidget` | widgetId, expectedRevision, operationId, target | Tombstone et annulation de publication, idempotent. |
| `RefreshProjectWidget` | widgetId, expectedRevision, requestId, target | Admission rapide → refreshId/statut. Le RPC n’attend pas la fin de l’agent. |
| `CancelProjectWidgetRefresh` | widgetId, refreshId, requestId, target | Invalidation durable et demande d’arrêt ciblée ; annulation répétée sans effet supplémentaire. |

Éviter un `Mutate { op: arbitrary-json }` pour ce domaine : les invariants CAS/idempotence doivent être visibles dans les DTO. Valider espace encore vivant, appareil owner, profil et révision au moteur destinataire ; `targetDeviceId` est du routage, pas une autorisation. Un ID widget lu sur un autre owner ne donne aucun accès à son filesystem.

MCP V1 : `list_widgets {project?,device?}`, `get_widget {widgetId,device?}`, `create_widget {project,creationKey,operationId,title,script,assets?,refreshRecipe?}`, `update_widget {widgetId,expectedRevision,operationId,...,refreshId?}`. La résolution projet réutilise celle de `Zeron::resolve_target`/`resolve_space_in`, y compris erreurs d’ambiguïté. Dans un chat de projet, omission du projet pour la liste peut utiliser cet espace ; hors chat, projet requis pour écrire, jamais « premier projet » implicite.

`get_widget` retourne la révision et, sur demande explicite, le script/manifest/assets dans des limites documentées ; pas toutes les données de tous les widgets dans `list_widgets`. Pour permettre un update sans perdre d’assets, une mise à jour de contenu fournit un bundle complet, ou une référence **exacte** au bundle courant validée par le moteur pour recopier les assets inchangés. Ne jamais interpréter omission comme « efface tous les assets » par hasard. Une modification de titre seule n’exige pas de relire le bundle.

Les chemins d’assets MCP sont lus dans le cwd du subprocess, comme pour `show_visual`, puis envoyés sous forme de bytes. Le moteur ne doit pas lire le cwd d’un agent distant. Les réponses incluent `widgetId`, `revision`, `contentRevision`, `created/replayed/staged`, et des erreurs stables telles que `widgets.conflict`, `widgets.key_reused`, `widgets.not_found`, `widgets.owner_unavailable`, `widgets.refresh_inactive`, `widgets.invalid_visual`.

Pas d’outil agent delete/reorder/refresh dans le premier catalogue : ils ne sont pas nécessaires pour lister/lire/créer/mettre à jour et favoriseraient notamment le refresh récursif. Le module moteur expose déjà ces actions à l’UI ; leur éventuelle exposition future passera par le même catalogue et les mêmes règles. `show_visual` reste une publication éphémère de chat ; il ne crée jamais implicitement de widget. Un agent n’enregistre un widget que sur demande ou dans le parcours produit explicite prévu.

### 6.2 Interface de réglages MCP

DTO proposés dans `crates/proto/src/mcp.rs` :

- `McpToolDescriptor` : nom stable, description originale, schémas et contraintes contextuelles intrinsèques ; forme possédée sans lifetime statique dans les réponses.
- `McpToolOverride` : nom, `enabled?` et `descriptionOverride?`. Omission = suivre le défaut, pas une copie persistée de la valeur originale.
- `McpCatalogSnapshot` : `catalogVersion`, `policyRevision`, liste complète des descriptors + état effectif et raisons de disponibilité ; Settings reçoit les originaux même si désactivés.
- Fichier versionné `{version, revision, tools:{name:{enabled?,descriptionOverride?}}}`. Conserver sans activer les overrides de noms absents après downgrade ; ne jamais leur permettre de créer un outil. Une suppression de champ remet au défaut.

| RPC proposé | Rôle |
| --- | --- |
| `WatchZeronMcpCatalog {targetDeviceId?}` | Snapshot d’inspection complet puis changements de politique/catalogue ; état pour Settings, pas de schémas dupliqués en UI. |
| `SetZeronMcpToolOverride {name, expectedPolicyRevision, enabled?, descriptionPatch, operationId, targetDeviceId?}` | Patch atomique ; descriptionPatch distingue unchanged/set/reset. Retour de la politique réellement persistée, conflit si stale. |
| `GetZeronMcpDiscovery {originChatId?, bridgeVersion}` | Local au bridge : catalogue effectif filtré par politique **et** contexte réel, état/version. Settings n’utilise pas ce catalogue filtré. |
| `AdmitZeronMcpToolCall {name, originChatId?, bridgeVersion}` | Local au bridge : admission courante, sous l’owner de politique ; refus stable si outil désactivé, contexte invalide ou politique illisible. Aucun cache d’autorisation dans le bridge. |

Deux dernières méthodes non relay-forwardables : les outils d’un bridge injecté suivent la politique du moteur qu’il joint ; ensuite leurs opérations peuvent viser un autre appareil via les RPC habituels. Settings distant utilise les deux premières méthodes pour régler le moteur explicitement choisi. La politique de B ne filtre pas arbitrairement la liste du MCP hébergé par A ; elle filtre les bridges attachés à B. Expliquer ce point dans la documentation technique sans charger le parcours produit.

## 7. Rafraîchissement : exécution, succès et annulation

Le refresh est orchestré par le moteur, jamais par le widget Luau ni par la durée de vie de la fenêtre. Un widget sauvegardé sans recette demeure consultable ; Actualiser propose de configurer la recette.

1. Préflight : espace vivant et owner en ligne, recette valide, harness installé/activé, moteur capable de servir le MCP, outils de lecture/update widgets autorisés. La disponibilité effective des intégrations externes est vérifiée par l’agent ; ni Settings MCP Zeron ni un faux probe global ne la garantissent. Une erreur de préflight ne touche pas le contenu.
2. Admission transactionnelle : fixer `refreshId`, baseRevision, requestId et état `queued`. Réutiliser une tentative active existante pour le même widget, même après timeout réseau du client. Persister **avant** de lancer le run.
3. Utiliser un chat de refresh ordinaire, dédié au widget, distinct du chat source. Le créer au premier refresh, puis le réutiliser quand il est idle ; le recréer s’il a été supprimé. Le garder archivé hors de la liste courante avec lien « Ouvrir l’exécution », en réutilisant l’archivage existant. Attention : `queue_command_with_transfers` désarchive actuellement les chats envoyés ; prévoir une voie interne de commande de refresh qui préserve explicitement l’archive, tout en gardant le comportement utilisateur par défaut. Ne pas introduire une nouvelle hiérarchie parent/enfant ou un système de contexte agent pour cela. Ne jamais steer/interrompre un chat utilisateur occupé pour actualiser un dashboard.
4. Envoyer une commande Run avec commandId durable et recette, widgetId/baseRevision/refreshId, et contenu de référence borné. Le lien commandId → tentative doit être persisté avant drain. Réutiliser le ledger `DocHost` ; traiter le crash entre admission, création chat et enqueue avec une outbox locale de tentative et des IDs préalloués, pas un second lancement aveugle. Comme `queue_command` génère aujourd’hui l’ID en interne, ajouter une entrée moteur interne qui accepte un ID préalloué et rejoue idempotemment le même payload ; l’entrée utilisateur actuelle garde ses defaults.
5. Le harness garde ses intégrations/configuration/permissions normales. Il collecte les données puis appelle `update_widget` pour ce widget. Le moteur vérifie le chat d’origine, la tentative active, la révision et l’exécution corrélée ; il valide le bundle avec le runtime existant et stocke un **candidat**. Un appel sans refreshId depuis ce chat ne peut pas contourner le staging et publier directement.
6. Lorsqu’un candidat plus récent est soumis dans la même tentative, le remplacer dans le staging avec operationId propre et attemptRevision ; seul le dernier candidat validé est éligible. Un candidat invalide ne remplace jamais le dernier valide. Pour une exécution de refresh, limiter l’écriture à son widget ; le reste demeure soumis aux règles normales de l’agent et n’est pas obtenu par une permission supplémentaire implicite.
7. Publier seulement après fin réussie du **tour corrélé**, candidat présent et tentative active. Faire en une transaction : vérifier baseRevision/tombstone/refreshId, changer contentRevision, incrémenter revision, lastSuccessfulAt, état succeeded. Un « tour terminé sans update » est une erreur `no_result`, pas un succès.
8. Erreur d’outil/agent, timeout, réponse abandonnée, annulation, suppression, conflit CAS : état terminal approprié, contenu précédent inchangé. Une tentative `awaitingInput` est visible et reprend seulement après réponse ; délai global borné proposé de 15 minutes, avec erreur de timeout puis arrêt ciblé. Distinguer l’heure du dernier succès du dernier essai et de l’âge des données affichées par le visuel.

États de tentative : `queued → running → awaitingInput → running → succeeded`, avec sorties terminales `failed`, `cancelled`, `interrupted`, `conflict`. `validating`/`cancelling` peuvent être des phases visibles, sans servir de vérité pour l’acceptation d’un résultat. La date du dernier succès initial est celle de la création/sauvegarde validée ; ne pas l’afficher comme date de collecte garantie si le visuel est plus ancien.

**Corrélation à compléter :** `Session.last_completed_turn` existe déjà et conserve le dernier ID d’entrée assistant achevée avec succès ; `DoneStatus` distingue completed/interrupted/errored. Réutiliser ce marqueur, mais ne pas confondre le dernier tour réussi avec celui du commandId de refresh courant. Un statut idle ou un changement d’horodatage ne suffit pas, et le transcript peut arriver après le statut. Le journal de `SessionsEngine::subscribe` donne seq/event ; ajouter au seam moteur/DocHost le rattachement commandId → run/tour et sa notification terminale fiable. Privilégier un reçu moteur local si cela évite un nouveau champ de wire inutile ; tout champ sérialisé ajouté reste compatible. Une session persistante se parque entre tours : attendre la fin du tour, jamais la sortie du process. Le callback `TurnListener` existant ne convient pas. Conserver les callbacks diff existants, ne pas les détourner. Tester les transitions par cette interface publique, sans dépendre d’un texte de réponse de l’agent.

**Annulation :** dans une transaction, rendre la tentative non publiable avant de demander l’interruption. L’ack de Cancel signifie « aucun nouveau résultat de cette tentative ne pourra être publié », même si un subprocess tarde à s’arrêter. Ensuite utiliser `interrupt/stop_turn` du chat dédié et son corrélat commandId pour ne pas arrêter une tentative suivante ; réutiliser `CommandBasedOn.turn_id` lorsque la commande Interrupt traverse le ledger, avec validation de tentative au coordinateur. Si la fin réussie a déjà gagné la transaction, Cancel retourne `alreadyCompleted`, sans mentir sur un rollback inexistant. Une nouvelle tentative obtient un nouveau refreshId. Les callbacks/updates tardifs échouent sans modifier le widget.

La fermeture de la page n’annule pas le refresh moteur ; la suppression du widget l’annule. Un redémarrage moteur marque les tentatives non terminales interrompues avant de reprendre les drains ; invalider leur capacité de publication même si un ancien command/run est retrouvé. `recover_stale` est actuellement appelé tôt dans l’assemblage et peut relancer les runs abandonnés : ouvrir/réconcilier l’index de tentatives avant cette récupération et lui fournir l’exclusion des chats/commandes de refresh concernés. Maintenir la récupération habituelle des autres chats. Pas d’auto-retry d’une requête analytique payante. Diagnostics limités à IDs, phases, durée et code d’erreur, sans données analytiques ni tokens.

## 8. Politique MCP : discovery, dispatch et sessions

### 8.1 Application effective

`Tools::list/has` deviennent des opérations qui obtiennent la politique courante par RPC (ou utilisent un snapshot uniquement pour l’affichage, jamais pour admettre). `jsonrpc::handle_request` attend la discovery. `Tools::call` possède sa **propre** garde d’admission avant parse, lecture de fichiers, résolution projet et effet de bord ; le contrôle de `has` dans JSON-RPC ne suffit pas, car des appelants internes/tests peuvent appeler `call` directement.

Le point de linéarisation est l’admission moteur : un appel admis avant la désactivation est déjà en vol et peut finir. Dès que la sauvegarde de désactivation est acquittée, toute admission suivante du nom échoue, y compris depuis une liste ancienne. Ne pas promettre l’annulation rétroactive de `send_message` déjà accepté. Aucun retry automatique d’une admission dont le transport a un résultat incertain ne doit dupliquer un effet métier.

Les batches sont un cas obligatoire : désactiver `create_chat` interdit aussi sa composition dans `create_chats` ; idem `send_message` dans `send_messages`. Le batch admet son propre nom puis chaque opération enfant passe par la même garde, au moment où elle commence. Pas d’appel direct au handler privé qui contourne la règle. À l’inverse désactiver seulement le nom batch n’interdit pas l’outil unitaire encore activé. Les permissions sous-jacentes du moteur restent appliquées.

Une description override est du texte de catalogue, jamais une instruction privilégiée pour le moteur. Elle ne change ni nom, schéma, contraintes « uniquement en chat », sandbox, permissions ni dispatch. Valider UTF-8/longueur (proposition 16 Kio), refuser vide/blanc et caractères de contrôle inappropriés ; conserver la longue description originale de `show_visual` sans la tronquer. Une limite d’override ne s’applique pas rétrospectivement aux descriptions natives existantes.

Écriture de politique : validation et CAS sous owner, fichier temporaire adjacent + remplacement atomique, publication de la nouvelle revision seulement après persistance réussie. Échec disque = réglage non appliqué, message clair et état précédent conservé. Fichier absent = defaults historiques. Fichier corrompu/version inconnue = ne pas repartir silencieusement avec tous les outils activés ; garder le dernier état valide en mémoire, ou au boot rendre le MCP indisponible jusqu’à réparation explicite depuis Settings. Préserver le fichier pour diagnostic, sans loguer les overrides complets.

La portée de cette fonction est l’offre d’outils du MCP Zeron. Elle ne retire pas les fonctions UI correspondantes, ni les droits généraux de filesystem/shell d’un agent ; les RPC UI restent disponibles. Ne pas présenter ce réglage comme une isolation de sécurité d’un agent pouvant déjà agir autrement.

### 8.2 Catalogues mis en cache et versions

- `tools/list` retourne toujours la politique et le contexte actuels pour un bridge compatible. Désactivation : l’outil disparaît. Réactivation et description modifiée : la prochaine discovery reflète le nouvel état.
- V1 recommandée : conserver honnêtement `listChanged:false` et signaler la reconnexion possible, plutôt que promettre une notification que tous les adapters ne traitent pas. L’enforcement à chaque call ne dépend pas du rafraîchissement de liste. Ajouter `notifications/tools/list_changed` ensuite seulement avec abonnement moteur, writer stdio, lifecycle et preuves adapter par adapter.
- Fournir `policyRevision`, `catalogVersion` et version de bridge dans les échanges internes pour diagnostiquer une session en retard. L’UI peut indiquer « liste à actualiser » si une session connue expose une révision antérieure, sinon « peut nécessiter une reconnexion », sans inventer une confirmation de mise à jour.
- Pi enregistre ses outils au démarrage du runtime ; au minimum une relance sûre de ce runtime est nécessaire pour voir les nouveaux noms/descriptions. Pour Claude/Codex/ACP/Cursor/OpenCode, vérifier le comportement des versions réellement installées ; ne pas supposer que reprendre un chat relit sa liste ou modifie ses descriptions.
- Aucune relance forcée en cours d’outil, aucun `terminate` automatique pour un changement de texte. La désactivation effective via admission s’applique sans cette relance sur les bridges compatibles.
- Distinguer **vieux catalogue dans un bridge compatible** (cas couvert sans délai) et **ancien binaire MCP antérieur à la fonctionnalité** (il ne contient aucune garde). Ne pas prétendre rendre ce dernier sûr avec un simple changement de liste. À la mise à niveau, imposer le renouvellement des bridges gérés par Zeron avant de déclarer leurs réglages pris en charge ; les runtimes persistants doivent participer à la vérification de version au prochain point sûr. Un `zeron mcp` ancien lancé extérieurement doit être remplacé/reconnecté ; documenter cette limite de compatibilité, sans assimiler le contrôle MCP à une ACL globale des anciens RPC.
- Ajouter les capabilities proposées `project_widgets_v1` et `zeron_mcp_tool_policy_v1` dans les infos d’engine/appareil pertinentes. Ancien moteur : Settings annonce fonctionnalité non prise en charge, mutations indisponibles ; pas de faux succès local. Bridge récent sur moteur trop ancien : erreur claire pour cette fonctionnalité, pas de fallback qui ignore une politique revendiquée. Si un mode legacy d’outils historiques est conservé, il doit être explicitement identifié non géré, jamais affiché comme contrôlé.

Le comportement protocolaire ci-dessus suit la [spécification MCP tools 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) déjà reconnue par le serveur actuel : capacité `listChanged`, notification optionnelle associée, distinction erreurs JSON-RPC et erreurs d’exécution. Un outil connu mais désactivé rend une erreur métier stable `mcp.tool_disabled` avec `isError:true` ; nom inconnu garde l’erreur protocole. Une annulation de refresh utilise son RPC métier, sans prétendre que les [notifications d’annulation MCP](https://modelcontextprotocol.io/specification/2025-06-18/basic/utilities/cancellation), actuellement ignorées, arrêtent un run.

### 8.3 Articulation avec les nouveaux outils widgets

Les quatre outils widgets entrent dans le catalogue canonique dès leur ajout, avec les mêmes defaults et overrides que les autres. Pas de switch caché spécifique aux widgets. Si `update_widget` est désactivé, un refresh ne peut pas le contourner par un RPC réservé ou `show_visual` ; préflight et dispatch signalent le blocage. Si la désactivation arrive pendant le refresh, toute nouvelle soumission est refusée. Un candidat déjà admis/validé peut finir selon la règle des appels en vol ; afficher cette sémantique sans promettre un rollback. Le bouton Annuler reste disponible par l’UI.

Le réglage de `show_visual` n’interdit pas l’affichage d’un widget déjà sauvegardé ; sa désactivation interdit les nouvelles publications via cet outil. Les droits de lecture/édition depuis la page projet demeurent ceux de l’UI et du moteur. L’activation d’un outil widgets ne lui accorde aucun accès supplémentaire aux intégrations PostHog/GA.

## 9. Étapes et commits prévus — une seule branche

Tous les commits futurs ci-dessous sont sur `feat/project-widgets-mcp-settings`, dans le worktree déclaré. Ils constituent une série intégrée, pas deux branches empilées. Les intitulés sont indicatifs. Aucun n’est créé par cette tâche.

### Étape 1 — Catalogue et contrats partagés

Commit proposé : `refactor(mcp): expose canonical tool descriptors and policy contracts`.

- Extraire catalogue et descriptions vers `crates/mcp/src/catalog.rs`, conserver exactement schémas/noms/comportements actuels ; exporter des définitions possédées pour l’override effectif.
- Ajouter DTO MCP et capabilities dans `crates/proto/src/mcp.rs`, `workspace.rs`, exports ; méthodes dans `crates/rpc/src/lib.rs`. Mettre à jour la dépendance moteur après contrôle du graphe.
- Définir disponibilité intrinsèque et mapping nom→handler exhaustif ; pas de copy/paste du catalogue dans Settings ou moteur. Distinguer schema version et policyRevision.
- Acceptance : catalogue par défaut identique, `show_visual` toujours absent hors chat, noms uniques, tous les handlers couvrent exactement les descriptors, round-trip DTO et defaults d’ancien EngineInfo.
- Vérifications prévues : tests catalogue/proto et `zeron-mcp` ; contrôle ciblé du graphe de crates et des exports. Pas de build applicatif à cette étape.

### Étape 2 — Persistance et admission de la politique MCP

Commit : `feat(engine): persist and enforce Zeron MCP tool policy`.

- Ajouter `crates/engine/src/mcp_settings.rs`, owner dans EngineCore et EngineRpc, fichier profil, getters/watch/setters/admission, erreurs typées, révision atomique.
- Modifier `Tools::list/has/call`, `jsonrpc::handle_request`, `Zeron` pour discovery/admission courantes ; garder le protocole annoncé cohérent.
- Faire passer les opérations unitaires des batches par la garde. Définir migration des bridges persistants et détection de support ; ne pas garantir l’application à un vieux binaire sans renouvellement.
- Acceptance : un outil désactivé disparaît de discovery et échoue avant effet, même depuis vieux catalogue ou appel direct Tools::call ; override respecte schéma/permissions ; modification vue par tous les bridges compatibles du moteur ; pas de perte interprofil.
- Vérifications : tests en mémoire à deux connexions MCP contre le même owner moteur, désactivation entre list/call, admission concurrente avec sauvegarde, batch/unitaires, échec IPC/persistance, redémarrage, corruption et réinitialisation. Vérifier qu’une description seule ne modifie aucun schéma sérialisé.

### Étape 3 — Settings MCP utilisable

Commit : `feat(ui): inspect and configure built-in Zeron MCP tools`.

- Ajouter `crates/ui/src/settings/mcp.rs`, module Settings, `SettingsSection::Mcp` et branche de rendu/navigation/focus dans `shell.rs`, slug persistant compatible dans `settings.rs`.
- Reprendre sélecteur d’appareil et patterns de tâches `AccountsPage`/`HarnessesPage`, primitives `settings/widgets.rs`. Lire et écrire via le moteur ; ne pas écrire les fichiers de préférences depuis l’UI.
- Recherche, détails/schémas, original/reset, brouillon, erreurs, affichage de la portée et note sur reconnexion. Garder le brouillon si conflit/échec ou changement d’appareil annulé explicitement.
- Acceptance : édition/reset/toggle indépendants, anciens moteurs read-only/non pris en charge, tous outils désactivables, Settings encore opérable, aucune perte de focus ni écriture sur mauvais appareil après réponse tardive.
- Vérifications : tests de reducers/contrats et focus utiles ; fixture visuelle pour liste longue/description longue/schéma ; revue native ultérieure seulement sur demande de preview, pas automatiquement.

### Étape 4 — Store widgets, bundles et RPC

Commit : `feat(engine): persist project widgets with immutable assets and revision checks`.

- Ajouter les DTO et méthodes de §6.1, store SQLite/migrations, bundles et GC conservateur ; assembler owner dans EngineCore/EngineRpc.
- Sauvegarde d’une pièce source, create/update/reorder/delete, déduplication et CAS, watchers et lecture chunkée, routing owner et contrôle espace/profil.
- Traiter DeleteChat sans cascade widget et DeleteSpace avec réconciliation owner ; n’élargir aucune jail d’uploads. Ne pas modifier le protocole registry pour synchroniser des données qui ne le sont pas.
- Acceptance : sauvegarder puis supprimer la source laisse script/assets lisibles après redémarrage ; deux clients ne perdent aucun update silencieusement ; retry n’ajoute aucun doublon ; bundle jamais visible incomplet ; remote owner et mauvais profil refusés correctement.
- Vérifications : tests du module sur store temporaire et deux clients RPC ; crash/fault injection autour copie→rename→transaction ; assets manquants/traversal/symlink/tailles, disque plein, reorder concurrent create/delete, tombstone et restart. Tests focused de forwarding/stream cleanup. Pas de nouveaux tests qui recopient simplement la liste des champs.

### Étape 5 — Outils agents widgets soumis à la politique

Commit : `feat(mcp): add idempotent project widget read and write tools`.

- Ajouter les quatre descriptors/handlers du §6 au catalogue unique et aux adaptateurs Zeron ; partage de la lecture/validation des fichiers de `show_visual` sans coupler leurs effets.
- Documenter creationKey/operationId, révision attendue, patch metadata vs bundle, conflits et nécessité de relire ; aucune déduplication par titre. Inclure outils nouveaux dans Settings automatiquement.
- Acceptance : créer via agent, lister/lire, updater avec révision, refuser révision périmée et rejeu divergent ; disable `update_widget` produit le même refus que pour les autres outils. Un appel sans projet identifiable ne crée rien.
- Vérifications : tests MCP contre moteur/RPC réels en mémoire et assets temporaires ; isolation owner/profil et ambiguïtés de noms ; catalogue comporte les nouveaux schémas sans casser show_visual.

### Étape 6 — Sauvegarde et espace projet Widgets

Commit : `feat(ui): save chat visuals and browse project widgets`.

- Ajouter `crates/ui/src/project_widgets.rs` et route `Route::ProjectWidgets` avec état espace explicite ; entrée menu projet/sidebar et retour de navigation. Intégrer l’action de sauvegarde à `TranscriptView::render_visual` dans le chrome de la ligne, pas dans `ui.*` Luau.
- Adapter `VisualFrame` à la source/seed explicites sans régression inline, cache par révision et libération de ressources. Liste, rename, reorder clavier/drag, delete, lien source et états complets.
- Acceptance : parcours PostHog/GA sauvegardé depuis une pièce historique hors run, indépendant du chat et de sa suppression ; accès sans chat sélectionné ; plusieurs espaces du même dépôt explicitement distingués ; aucune collision avec canvas/composer/#882.
- Vérifications : fixtures contenu analytique, visuel avec assets, liste longue et erreurs ; tests de navigation/focus pertinents, réponse tardive de mauvais espace, chargement/hors ligne/vide distincts. Lorsqu’une preview sera demandée : petit/grand viewport, thèmes et reduced-motion selon `zeron-ui-evidence` ; aucune capture/build automatique dans ce plan.

### Étape 7 — Refresh manuel moteur et commandes UI

Commit : `feat(widgets): refresh through cancellable agent runs and keep last success`.

- Ajouter coordinateur et états durables, recette et dialogue minimal, outbox/corrélation commandId-tour, chat dédié et staging avec `refreshId`. Étendre le seam de fin de tour seulement si nécessaire ; ne pas modifier le contexte des agents.
- Brancher lancement/annulation sur les primitives DocHost/Sessions ; régler crash/restart, awaitingInput, timeout, fin sans résultat, candidat invalide, disparition d’espace/chat, conflit de révision et publication tardive.
- Acceptance : un seul run par clic rejoué, succès après tour réussi seulement ; échec/annulation conserve bytes et lastSuccessfulAt ; double refresh et annulation croisée ne touchent pas le nouveau run ; fermeture de vue ne perd pas le job ; disable de l’outil d’update est respecté.
- Vérifications : harness mock déterministe avec barrières d’exécution pour tester les races ; candidat puis erreur terminale, suppression/conflit pendant validation, annulation avant/après commit, restart avant/après enqueue, awaitingInput et timeout. Test d’intégration utilisateur PostHog/GA avec intégrations réelles uniquement lors d’une validation ultérieure explicitement autorisée ; pas nécessaire pour les invariants CAS/annulation.

### Étape 8 — Compatibilité et documentation de livraison

Commit : `docs(widgets): document ownership refresh and MCP session compatibility` (avec petits correctifs de compatibilité si révélés).

- Mettre à jour `docs/mcp.md`, ajouter documentation widgets (et état de ce plan), préciser profils, absence de réplication offline, import, schémas/erreurs, defaults et reconnexion. (Vérifié : à HEAD, aucun commentaire de `engine`, `proto`, `mcp` ou `doc` ne mentionne encore HTML pour `PublishVisual` ; rien à corriger de ce côté.)
- Refaire la revue ciblée de #882 si la base a changé ; garder la route Widgets dédiée. Ne pas importer les autres travaux en cours.
- Acceptance : matrice ancien/nouveau moteur-client-bridge documentée ; migrations additives, downgrade sans suppression de données ; limitation des anciens binaires explicite ; scénarios clés vérifiés et limites déclarées.
- Vérifications finales prévues : `cargo fmt --all -- --check` (juger seulement les hunks de la branche si drift upstream), tests ciblés `zeron-proto`, `zeron-mcp`, modules moteur concernés puis `zeron-ui` ciblé ; `cargo check` des packages affectés. Élargir seulement si un risque restant le justifie. Les opérations lourdes requièrent alors `zeron-storage-watcher`. Aucun de ces checks n’est exécuté lors de la rédaction de ce plan.

Ordre de dépendances : **1 → 2 → 3** pour le contrôle ; **1 → 4 → 5 → 6 → 7**, avec **2 et 5 requis avant 7** ; **8** clôt l’ensemble. La UI widgets de 6 peut être conçue dès 4, mais aucun merge externe ou seconde branche n’est nécessaire. Les contrats communs sont stabilisés dans 1/4 et exercés par tous les appelants.

## 10. Vérifications transversales et critères de sortie

Le premier périmètre est terminé seulement si les scénarios suivants passent :

1. Dans un chat de projet, afficher un dashboard avec un asset ; le sauvegarder deux fois depuis deux vues ; un seul widget. Fermer puis supprimer le chat, redémarrer : visuel identique et lien source absent traité proprement.
2. Renommer/réordonner/supprimer depuis deux clients connectés au même owner : conflits explicites, aucun update perdu ni widget ressuscité par retry.
3. Exécuter un refresh mock puis réel autorisé avec données PostHog/GA : nouveau contenu au succès ; une panne d’intégration, erreur de rendu, fin sans candidat, annulation ou crash laisse le dernier contenu/date de succès intact. Le résultat de N ne peut pas écraser N+1.
4. Depuis un autre appareil autorisé, voir/modifier les widgets de l’owner en ligne ; owner éteint : état indisponible et données déjà affichées conservées. Aucun faux cache cloud ni envoi dans un autre profil.
5. Inspecter tous les outils Zeron, dont les quatre outils widgets, voir schémas/original, override puis reset et redémarrer : état conforme ; descriptions ne changent pas les droits.
6. Lister un outil, le désactiver depuis Settings, appeler via ce vieux catalogue sur un bridge compatible : refus avant effet. Idem via `Tools::call` direct et batch ; tout désactiver puis réactiver depuis Settings reste possible.
7. Politique illisible ou moteur injoignable : pas de retour aux outils activés par défaut en prétendant qu’ils sont contrôlés. Ancien moteur/bridge explicitement non pris en charge ou renouvelé, aucun faux indicateur « appliqué ».
8. Native UI utilisable clavier/petites tailles/grande typographie et thèmes ; absence de fuite de tâches/GPU quand on quitte le projet ou remplace le contenu. La présence éventuelle de #882 ne modifie pas l’accès aux widgets.

Aucun build, test de fonctionnalités ou agent analytique n’a été exécuté pour valider ces comportements dans cette tâche : ce sont les critères de l’implémentation future. Validation de ce livrable : cohérence avec les sources inspectées, liens/chemins existants, `git diff --check`, et contrôle que seul ce plan a été ajouté.

## 11. Migrations et compatibilité

- Stores nouveaux et versionnés : absence = aucun widget / réglages historiques ; migrations SQLite append-only transactionnelles. Version future inconnue : désactiver les écritures sans écraser le store. Fichier MCP illisible : récupération explicite et fail-closed du MCP géré, pas de reset automatique permissif.
- Pas de migration des pièces `visual` existantes : elles restent dans leurs chats jusqu’à sauvegarde explicite. Conserver les noms d’outils existants, le comportement inline de `show_visual` et les limites courantes.
- Ne pas supprimer les chemins/uploads legacy autorisés lors de la copie ; le nouveau bundle devient indépendant. Aucun accès au cache d’un autre profil.
- Les nouveaux DTO optionnels ont des defaults/skip appropriés ; nouvelles capabilities pour toutes les mutations. Ne pas sérialiser un nouveau statut obligatoire dans des rows existantes si les anciens clients ne le tolèrent pas ; les états widgets restent dans leur propre contrat.
- Retour à un ancien binaire : il ignore les nouveaux stores et ne doit pas les effacer. Les nouveaux widgets ne sont alors pas exposés par l’ancien client. Le contrôle MCP n’est pas rétroactivement garanti par l’ancien serveur ; afficher/documenter ce fait lors d’un downgrade et exiger un bridge compatible pour la fonctionnalité.
- Import local→synced : recommandé ne pas copier automatiquement widgets/recettes/overrides lors du premier périmètre. Le mécanisme existant importe des sessions, pas ce nouveau store. Afficher cette limitation dans l’aide d’import si nécessaire ; conserver le store local intact. Une future option d’import devra remapper spaceId, provenance, bundles et clés, sans importer des overrides silencieusement.
- Pas de changement edge/registry ni de déploiement nécessaire pour la portée host-local recommandée. Ne pas ajouter des tables vides « pour la future sync ».

## 12. Risques réels et décisions produit encore ouvertes

| Décision / risque | Recommandation et raison | Quand trancher |
| --- | --- | --- |
| « Projet » = dépôt logique ou espace hôte ? | V1 espace/appareil explicite. Le moteur a déjà ce propriétaire ; regrouper des clones n’élit pas un endroit où courir les intégrations. | Avant étape 4 ; défaut de ce plan. |
| Offline multiappareil indispensable ? | V1 accès relay connecté ; la réplication durable est une extension avec protocole blobs/conflits. Ne pas la vendre comme acquise via le registry. | Avant étape 4 si le produit exige davantage. |
| Emplacement widgets | Route projet dédiée. Évite la concurrence avec #882 et garde l’accès quand le composer change ou qu’un chat est ouvert. | Design de 6 ; décision routinière proposée. |
| Chat de refresh et coût visible | Chat dédié réutilisé, archivé et accessible, agent/modèle visibles lors de la configuration ; pas de rejouage de chat source. | Étape 7. |
| Enregistrer automatiquement tous les visuels ? | Non. Action explicite ou création agent demandée, sinon pollution du projet et doublons. | Défaut produit. |
| Scope MCP device global ou profil ? | Profil + moteur joint ; overrides potentiellement propres au compte, pas de sync. Mention dans Settings. | Avant étape 2. |
| Tous les outils désactivables ? | Oui, car la récupération passe par les RPC UI ; pas de permission implicite pour `whoami`, create/update widgets ou batch. | Défaut produit. |
| Notification de catalogue live | V1 reconnexion honnête, admission live ; notification ultérieure seulement avec preuve harness. Ancien binaire ≠ ancien catalogue, renouvellement nécessaire. | Étape 2 et vérification 8. |
| Arrêt d’un process lent | Annulation durable coupe la publication d’abord ; l’arrêt du harness suit ses garanties existantes. Éviter de proclamer une annulation physique immédiate des requêtes externes. | Tests de 7. |
| Édition pendant refresh | CAS global conservateur : conflit explicite plutôt qu’écraser le renommage/recette ou faire une fusion implicite. | Défaut de 4/7. |
| Stockage volumineux et assets | Bundle immuable complet, borne actuelle, GC conservateur et quota proposé initial de 50 widgets par espace ; mesurer avant optimisation/déduplication globale. Échec quota/disque plein sans perte du dernier succès. | Quota avant 4 ; valeur ajustable. |
| Suppression d’espace synchronisée | Cascade widgets sur owner avec réconciliation, information utilisateur, aucun héritage vers un autre clone. | Avant 4/6. |
| Données analytiques sensibles | Profil isolé, aucune donnée brute/secret dans logs ou fichier projet, même modèle de confiance des pairs existant. La recette n’embarque pas de credential. | Invariant à toutes les étapes. |

Ces recommandations permettent d’avancer sans bloquer le plan sur les libellés, l’ordre des menus ou la forme des IDs. Les deux questions qui changeraient réellement l’architecture sont la propriété au niveau dépôt multiowner et la consultation offline depuis n’importe quel appareil ; les autres choix se résolvent dans le périmètre décrit.

## 13. Hors périmètre explicite

- Contexte/mémoire des agents, Voice Glyph, CI panels et diagnostic de contexte vocal.
- Scheduling, cron, refresh automatique au lancement/à l’ouverture, abonnements push analytics.
- Boutique de templates/plugins, création d’intégrations PostHog/GA ou gestion de leurs credentials dans Zeron.
- Gestion générale de serveurs MCP tiers : ajouter/supprimer/installer/authentifier/configurer un serveur, inspecter tous les MCP de chaque harness.
- Réplication cloud/offline des bundles, nouveau document CRDT widgets, résolution multiowner, import automatique entre profils ou copie entre appareils.
- Dashboard canvas libre, grille redimensionnable, persistance des interactions Luau, historique/version browser, partage public et collaboration intercomptes.
- Reprise automatique payante après crash, refresh récursif par agents et annulation rétroactive d’un effet externe déjà accepté.
- Refonte de #882, des Project Actions, du renderer Luau/GPU, de la queue de sessions ou des permissions générales des agents.
- Toute implémentation, commit, publication GitHub, build, preview ou exécution agent analytique dans cette tâche de planification.

## 14. Sources et méthode

Instructions inspectées : aucun `AGENTS.md` trouvé dans la chaîne des parents du worktree ou dans ses fichiers versionnés ; `.agents` est un lien vers les skills locaux partagés, uniquement lus. `CONTEXT.md`, `ARCHITECTURE.md`, `docs/mcp.md`, exemples de `docs/plans` et fichiers Rust/JS/edge cités ci-dessus ont servi de points de départ. Les comportements du code au SHA fixé priment sur les phrases historiques des documents.

Skills appliqués : `zeron-contribution-branch`, `codebase-design`, règles `zeron-rust`/`zeron-gpui` et `better-layout` pour le plan des interfaces, owners, cancellation et parcours. Aucun sous-agent lancé. Lectures Git/GitHub uniquement pour la base et le chevauchement ; aucun travail annexe relancé. Spécification MCP officielle consultée dans la version déjà supportée par ce serveur, sans migration de protocole proposée.
