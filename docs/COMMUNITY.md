# Suite communautaire : événements, stickers, avatar de serveur, bannière

Contrat API (JSON-RPC + événements WebSocket) de la suite communautaire
introduite par D-047. Tout est **additif** : aucun contrat existant
(`groups.*`) n'est modifié dans son comportement historique, seulement
étendu — un client qui n'utilise aucun des champs/méthodes ci-dessous
continue de fonctionner à l'identique.

Les identifiants (`group_id`, `channel_id`, `event_id`, `role_id`, …)
transitent en **hexadécimal minuscule** (32 caractères pour 16 octets, 64
pour une clé publique), comme partout ailleurs dans l'API. Toutes les
opérations sont répliquées via le même mécanisme d'op-log signé que le reste
de `groups.*` (SPEC §6.2) : permissions vérifiées **à l'émission et au
rejeu**, quotas appliqués de façon déterministe (même résultat chez tous les
pairs honnêtes, indépendamment de l'ordre d'arrivée des ops).

Nouveaux discriminants d'op de groupe (`GroupOpBody`, prochain libre après
0x1F) :

| Opcode | Op | Permission |
|---|---|---|
| `0x20` | `EventCreate` | `MANAGE_CHANNELS` |
| `0x21` | `EventEdit` | `MANAGE_CHANNELS` ou auteur de l'événement |
| `0x22` | `EventDelete` | `MANAGE_CHANNELS` ou auteur de l'événement |
| `0x23` | `EventRsvp` | tout membre (sur son propre RSVP) |
| `0x24` | `StickerAdd` | `MANAGE_EMOJIS` |
| `0x25` | `StickerRemove` | `MANAGE_EMOJIS` |
| `0x26` | `SetMemberAvatar` | self-service uniquement (auteur = cible, aucun modérateur) |
| `0x27` | `PollVote` | tout membre (sur son propre vote) |
| `0x28` | `PollClose` | `MANAGE_CHANNELS` ou auteur du sondage |
| `0x29` | `PollCreate` | `VIEW`+`SEND` effectifs dans `channel_id` (comme envoyer un message — **pas** la simple appartenance) |
| `0x2A` | `PollDelete` | `MANAGE_CHANNELS` ou auteur du sondage |
| `0x2B` | `SetAutoModWords` | `MANAGE_CHANNELS` |
| `0x2C` | `SetChannelSlowmode` | `MANAGE_CHANNELS` |

Aucun nouveau bit de permission n'a été introduit : les événements,
l'édition de la bannière et AutoMod réutilisent `MANAGE_CHANNELS` (même
famille que `SetMeta`/salons/catégories), les stickers réutilisent
`MANAGE_EMOJIS` (même famille que les émojis de serveur). L'avatar de
serveur ne dépend d'aucune permission — il est strictement self-service,
comme un pseudo qu'on se fixerait à soi-même sans jamais pouvoir agir sur
celui d'autrui.

`GroupOpBody::SetMeta` (0x02, inchangée dans son usage historique — nom +
icône) gagne un champ **additif** de fin de variant : `banner_color:
Option<u32>`. Un ancien pair qui ne le connaît pas ne l'écrit simplement pas
et le décode à `None` (rétrocompatibilité filaire, `Reader::opt_tail` —
même schéma que `CoreMsg::Profile.pronouns/accent_color/banner_color`).

`MsgBody` (corps de message de salon, `accord-proto`) gagne un nouveau
discriminant : `Sticker { name, merkle_root }`, **kind = 4** (jamais utilisé
jusqu'ici, entre `Reaction` = 3 et `Typing` = 5). Un pair qui ne connaît pas
ce discriminant échoue à décoder ce message précis (comportement identique à
tout autre corps futur non reconnu) sans affecter le reste du salon ni l'état
répliqué du groupe.

`MsgBody` gagne un second nouveau discriminant (D-048) : `Poll { poll_id,
question, options }`, **kind = 7** (prochain libre après `ReadReceipt` = 6).
Même dégradation gracieuse qu'un `Sticker` non reconnu chez un pair plus
ancien.

**Durcissement post-revue (D-048, avant tout déploiement)** : `PollCreate`
(0x29) portait initialement le seul `poll_id` et n'était gated que sur la
simple appartenance au groupe — deux failles corrigées avant que l'op ne
soit jamais répliquée en production :

- `PollCreate` gagne deux champs : `channel_id: [u8; 16]` (le salon où le
  message `MsgBody::Poll` associé est posté) et `msg_id: [u8; 16]` (le
  message canonique auquel `poll_id` est lié). Le repli exige désormais
  `VIEW`+`SEND` effectifs de l'auteur dans `channel_id` — exactement la même
  porte qu'un envoi de message ordinaire
  (`GroupState::can_send_message`/`accord_core::group::msg::require_send`) —
  au lieu de la simple appartenance : sans ce correctif, n'importe quel
  membre pouvait squatter les 25 emplacements de `MAX_POLLS` via un salon où
  il n'avait même pas le droit d'écrire, DoS permanent sur les sondages du
  reste du groupe.
- `msg_id` lie `poll_id` à un unique message canonique : à l'ingestion d'un
  `MsgBody::Poll`, si l'op-log local connaît déjà ce `poll_id` (un
  `PollCreate` a été replié), seul le message qui correspond exactement à
  cet auteur ET ce `msg_id` est stocké — un pair qui rejoue ce `poll_id`
  avec un autre auteur ou un autre `msg_id` (question/options forgées) ne
  crée ni un second sondage visible, ni ne « rehéberge » le dépouillement
  existant sur son propre contenu.
- Nouvel op `PollDelete` (0x2A, voir le tableau ci-dessus) : mirrors
  `EventDelete` exactement (auteur ou `MANAGE_CHANNELS`), c'est le seul
  moyen de récupérer un emplacement `MAX_POLLS` une fois un sondage créé.
  Exposé via `groups.polls.delete` (§6.1).
- Ordre d'émission côté nœud (`Node::group_send_poll`) inversé : `poll_id`/
  `msg_id` sont générés d'abord, puis `PollCreate` est authored et confirmé
  **avant** toute composition/diffusion du message — si l'op est refusée
  (plafond atteint, droit d'écriture refusé), rien n'est composé ni envoyé,
  au lieu d'un message posté référençant un sondage jamais connu de l'op-log.

---

## 1. Événements planifiés (`groups.events.*`)

Un événement planifié (`title`, `description`, `start_ms`, `channel_id`
optionnel, `author`, ensemble de RSVP) vit dans l'état matérialisé du groupe,
au même titre que les salons ou les rôles — répliqué par l'op-log, jamais de
stockage séparé.

### 1.1 Méthodes RPC

| Méthode | Paramètres | Résultat | Erreurs notables |
|---|---|---|---|
| `groups.events.create` | `{ group_id, title, description?, start_ms, channel_id? }` | `{ event_id: hex16 }` | `INVALID_PARAMS` (titre/description hors bornes) ; `APP_ERROR` « refusé : … » (permission, salon vocal inconnu, date hors bornes, plafond atteint) |
| `groups.events.edit` | `{ group_id, event_id, title, description?, start_ms, channel_id? }` | `{ ok: true }` | idem création ; `APP_ERROR` « refusé : événement inconnu » |
| `groups.events.delete` | `{ group_id, event_id }` | `{ ok: true }` | `APP_ERROR` « refusé : … » (ni auteur ni `MANAGE_CHANNELS`, événement inconnu) |
| `groups.events.rsvp` | `{ group_id, event_id, interested? }` | `{ ok: true }` | `APP_ERROR` « refusé : événement inconnu » |

- `description` absente ⇒ chaîne vide.
- `start_ms` est une échéance murale absolue (ms), **pas** une durée.
- `channel_id`, s'il est fourni (chaîne hex ou `null` explicite = absent),
  doit référencer un salon **vocal** existant du groupe ; toute autre valeur
  est refusée (aucun salon textuel/annonces comme lieu d'événement).
- `interested` par défaut `true` (RSVP « je suis intéressé·e » en un appel) ;
  `false` retire le RSVP local. Dédoublonné par `(event_id, membre)` — un
  second RSVP du même membre remplace simplement le précédent.
- L'édition (`groups.events.edit`) est une réécriture complète des champs
  modifiables (pas de fusion partielle) ; les RSVP existants sont conservés
  tels quels, quel que soit qui édite.
- Suppression d'un salon vocal référencé par un événement : l'événement
  survit, `channel_id` repasse à `null` (même politique que la suppression
  d'une catégorie, qui « décatégorise » ses salons plutôt que de les
  supprimer).

### 1.2 Événement WebSocket : `event.group_event_started`

Émis **localement** (jamais répliqué sur le réseau — chaque pair honnête
détecte le passage de l'échéance de son côté) quand l'heure de début d'un
événement est atteinte :

```json
{
  "group_id": "hex16",
  "event_id": "hex16",
  "title": "Soirée jeux"
}
```

- Vérifié toutes les 60 secondes (boucle de maintenance dédiée,
  `MaintenanceConfig::event_check`).
- Émis **une seule fois** par événement : le suivi des identifiants déjà
  signalés est persisté en base locale (métadonnée jamais répliquée) et
  survit à un redémarrage.
- Un événement dont l'heure de début est passée de **plus d'une heure**
  n'est jamais (re)signalé — y compris juste après un redémarrage qui aurait
  raté la fenêtre : pas de rattrapage bruyant d'événements anciens.

### 1.3 Forme dans `groups.state`

Chaque entrée du tableau `events` :

```json
{
  "event_id": "hex16",
  "title": "Soirée jeux",
  "description": "Amenez vos manettes.",
  "start_ms": 1700000000000,
  "channel_id": "hex16 ou null",
  "author": "hex32",
  "rsvp_count": 3,
  "rsvped": true
}
```

`rsvped` reflète l'appelant local (clé publique locale dans l'ensemble des
RSVP) — l'UI n'a pas besoin de connaître sa propre clé pour l'afficher.

---

## 2. Stickers de serveur (`groups.stickers.*`)

Mêmes règles que les émojis de serveur (`groups.emoji.*`), y compris le
mécanisme de stockage (image publiée dans le magasin de fichiers, référencée
par sa racine Merkle SHA-256 de contenu).

### 2.1 Méthodes RPC

| Méthode | Paramètres | Résultat | Erreurs notables |
|---|---|---|---|
| `groups.stickers.add` | `{ group_id, name, mime, data_b64 }` | `{ merkle_root: hex32 }` | `INVALID_PARAMS` (nom, MIME, taille, base64) |
| `groups.stickers.remove` | `{ group_id, name }` | `{ ok: true }` | `INVALID_PARAMS` (nom invalide) ; `APP_ERROR` « refusé : sticker inconnu » |
| `groups.stickers.list` | `{ group_id }` | `{ stickers: [{ name, merkle_root }] }` | — |

- `name` : 2-32 caractères `[a-z0-9_]` (mêmes règles qu'un nom d'émoji).
- `mime` : `image/png`, `image/jpeg`, `image/webp` ou `image/gif`.
- `data_b64` : image décodée non vide, ≤ 512 Kio.
- `groups.stickers.add` sur un nom déjà enregistré **remplace** l'image
  (pas d'erreur « déjà existant »), exactement comme les émojis.
- Les stickers apparaissent aussi dans `groups.state.stickers` (même forme
  que `groups.stickers.list`), pour éviter un aller-retour supplémentaire
  quand l'UI charge déjà l'état complet du groupe.

### 2.2 Envoi d'un sticker (`groups.send` étendu)

`groups.send` accepte désormais un paramètre `sticker` optionnel :

| Méthode | Paramètres | Résultat |
|---|---|---|
| `groups.send` (sticker) | `{ group_id, channel_id, sticker: "<nom>" }` | `{ msg_id: hex16 }` |

Quand `sticker` est présent, `text`/`reply_to`/`attachments` sont **ignorés**
— un appel ne peut pas mélanger texte et sticker. Sans `sticker`, le
comportement historique (message texte) est inchangé.

`sticker` doit référencer un nom **actuellement enregistré** dans
`groups.state.stickers` de ce groupe ; la racine Merkle transmise sur le fil
est **dérivée côté serveur** de ce registre, jamais fournie par l'appelant —
un client ne peut donc pas forger un couple `(nom, racine)` sans rapport
avec un sticker réellement enregistré. Un nom inconnu (ou plus enregistré)
rend `INVALID_PARAMS` (« sticker inconnu »).

Forme du corps dans l'historique (`groups.history`, `groups.history_around`,
`event.group_msg`) :

```json
{ "type": "sticker", "name": "wave", "merkle_root": "hex32" }
```

Un pair plus ancien qui ne reconnaît pas encore le discriminant filaire
`Sticker` (kind 4) échoue à décoder ce message précis — dégradation
identique à celle de tout futur corps de message non reconnu, sans impact
sur le reste du salon.

---

## 3. Avatar de serveur (`groups.set_member_avatar`)

Avatar **par serveur** (distinct de l'avatar de profil global,
`profile.set_avatar`) : self-service strict, aucun modérateur ne peut
l'imposer ni l'effacer pour un autre membre — contrairement au pseudo de
serveur (`groups.set_nickname`), qui autorise un modérateur `MANAGE_ROLES` à
agir sur un membre de rang inférieur.

**Choix d'implémentation** : nouvel op `SetMemberAvatar` (0x26) plutôt
qu'extension de `SetNickname` (0x1E). `SetNickname` porte une chaîne et
autorise un tiers modérateur ; l'avatar porte une racine Merkle optionnelle
et n'autorise **jamais** de tiers. Fusionner les deux aurait forcé chaque
appelant à porter deux champs sans rapport (chaîne + option de hash) et
aurait compliqué la vérification de permission (self-service pur pour l'un,
modération hiérarchique pour l'autre) dans une seule op. Un op dédié, sans
champ `member` (la cible est toujours implicitement l'auteur), reste
minimal sur le fil et place la garde « self-service uniquement » au niveau
le plus simple possible : il n'existe tout simplement aucun champ pour
désigner une autre cible.

### 3.1 Méthode RPC

| Méthode | Paramètres | Résultat |
|---|---|---|
| `groups.set_member_avatar` | `{ group_id, mime?, data_b64? }` | `{ avatar: hex32 ou null }` |

- `data_b64` présent ⇒ `mime` requis, image décodée non vide et ≤ 512 Kio,
  `mime` doit commencer par `image/` (borne plus permissive que les
  stickers/émojis, alignée sur `groups.set_icon`) ; l'image est publiée dans
  le magasin de fichiers et sa racine Merkle devient l'avatar.
- `data_b64` absent (ou paramètre omis entièrement) ⇒ efface l'avatar
  (`avatar: null` en résultat).
- Toujours self-service : la cible est l'identité locale, il n'existe pas de
  paramètre pour désigner un autre membre.

### 3.2 Forme dans `groups.state`

Chaque entrée de `members[]` gagne un champ `avatar` :

```json
{
  "pubkey": "hex32",
  "roles": ["hex16", "…"],
  "nickname": "…ou null",
  "avatar": "hex32 ou null",
  "timeout_until_ms": 0,
  "voice_muted": false,
  "voice_deafened": false
}
```

Un membre expulsé, banni ou qui quitte volontairement perd son avatar de
serveur (retiré de l'état, comme son pseudo et sa modération vocale).

---

## 4. Couleur de bannière de serveur (`groups.set_banner_color`)

Champ additif `banner_color: Option<u32>` (`0xRRGGBB`, ≤ 24 bits) sur
l'op `SetMeta` existante (celle qui porte déjà le nom et l'icône du groupe).
`groups.rename` et `groups.set_icon` **préservent** la couleur de bannière
courante (ils relisent l'état avant d'émettre `SetMeta`, exactement comme ils
préservent déjà l'icône lors d'un renommage, ou le nom lors d'un changement
d'icône).

### 4.1 Méthode RPC

| Méthode | Paramètres | Résultat | Erreurs notables |
|---|---|---|---|
| `groups.set_banner_color` | `{ group_id, color: entier 0xRRGGBB ou null }` | `{ ok: true }` | `INVALID_PARAMS` (`color` absent, ou > 0xFFFFFF) |

`color` est **requis explicitement** (absent ⇒ `INVALID_PARAMS` — intention
sans ambiguïté exigée) : un entier fixe la couleur, `null` l'efface.

### 4.2 Forme dans `groups.state`

Nouveau champ racine :

```json
{
  "banner_color": "entier 0xRRGGBB ou null",
  "...": "reste du contrat groups.state inchangé"
}
```

---

## 5. Journal d'audit (`groups.audit`)

Nouvelles entrées décodées (`kind` en libellé stable, `params` limités aux
champs utiles à une description humaine) :

| `kind` | `params` |
|---|---|
| `set_meta` | `{ name, icon, banner_color }` (le champ `banner_color` s'ajoute à la forme existante) |
| `event_create` | `{ event_id, title }` |
| `event_edit` | `{ event_id, title }` |
| `event_delete` | `{ event_id }` |
| `event_rsvp` | `{ event_id, interested }` |
| `sticker_add` | `{ name }` |
| `sticker_remove` | `{ name }` |
| `set_member_avatar` | `{ avatar }` (hex32 ou `null`) |
| `poll_create` | `{ poll_id, channel_id, msg_id }` |
| `poll_vote` | `{ poll_id, option_index }` |
| `poll_close` | `{ poll_id }` |
| `poll_delete` | `{ poll_id }` |
| `automod_set` | `{ word_count }` (nombre de mots dans la liste de remplacement, pas les mots eux-mêmes — voir §7) |

---

## 6. Sondages (`groups.polls.*`, D-048)

Un sondage est posté **comme un message** de salon (`groups.send` avec un
paramètre `poll`) : la question et les options y voyagent, content-adressées
à un `poll_id` frais généré côté serveur. Les **votes**, eux, ne sont **pas**
dans le message — ils vivent dans l'op-log de groupe
(`PollCreate`/`PollVote`/`PollClose`/`PollDelete`) pour que tous les pairs
convergent sur le même dépouillement, exposé en direct dans `groups.state`.

**Choix d'implémentation** : quatre ops plutôt que deux. Le repli de l'op-log
(`GroupState::fold`) est une fonction pure du journal signé — il n'a jamais
accès au contenu des messages. Pour que « quel `poll_id` existe », « qui a
le droit d'écrire dans quel salon » et « qui peut le clore » soient des
faits **vérifiables cryptographiquement** (et non simplement déclarés par un
vote non fiable — le premier votant n'est pas forcément l'auteur du sondage,
et lui faire confiance permettrait à n'importe quel membre de « voler » le
droit de clôture d'un sondage d'autrui), le nœud émetteur enregistre
automatiquement une op `PollCreate`
en même temps qu'il diffuse le message — jamais exposée comme méthode RPC à
part entière, c'est un détail d'implémentation de `groups.send`.

### 6.1 Méthodes RPC

| Méthode | Paramètres | Résultat | Erreurs notables |
|---|---|---|---|
| `groups.send` (sondage) | `{ group_id, channel_id, poll: { question, options: [string, ...] } }` | `{ msg_id: hex16, poll_id: hex16 }` | `INVALID_PARAMS` (question/options hors bornes) ; `APP_ERROR` « refusé : … » (droit d'écriture, plafond de sondages atteint) |
| `groups.polls.vote` | `{ group_id, poll_id, option_index }` | `{ ok: true }` | `APP_ERROR` « refusé : … » (sondage inconnu, clos, `option_index` hors bornes) |
| `groups.polls.close` | `{ group_id, poll_id }` | `{ ok: true }` | `APP_ERROR` « refusé : … » (sondage inconnu, ni auteur ni `MANAGE_CHANNELS`) |
| `groups.polls.delete` | `{ group_id, poll_id }` | `{ ok: true }` | `APP_ERROR` « refusé : … » (sondage inconnu, ni auteur ni `MANAGE_CHANNELS`) |

- `question` : 1-300 octets UTF-8, non vide, sans caractère de contrôle
  (hors `\n`/`\r`/`\t`) — bornes vérifiées **à la composition et au
  décodage filaire**, pas seulement au repli (le message n'a pas d'étape de
  repli comparable à l'op-log).
- `options` : 2 à 10 entrées, chacune 1-100 octets UTF-8, non vide, mêmes
  règles de caractères de contrôle que la question. Aucune vérification
  anti-usurpation (bidi/zero-width) contrairement à un pseudo ou un nom de
  sondage : un texte de sondage n'apparaît jamais dans une liste de membres.
- Quand `poll` est présent, `text`/`reply_to`/`attachments`/`sticker` sont
  **ignorés** — un appel ne peut pas mélanger texte, sticker et sondage.
  Sans `poll`, le comportement historique de `groups.send` est inchangé.
- `option_index` (vote) : choix unique — un second vote du même membre sur
  le même sondage **remplace** le précédent (pas d'accumulation),
  dédoublonné par `(poll_id, membre)` au repli, exactement comme un RSVP
  d'événement.
- N'importe quel membre peut créer, voir ou voter sur un sondage **du moment
  qu'il a `VIEW`+`SEND` effectifs dans le salon visé** — aucune permission
  élevée au-delà (contrairement aux événements/stickers qui exigent
  `MANAGE_CHANNELS`/`MANAGE_EMOJIS`) : un sondage a exactement les mêmes
  droits qu'un message ordinaire dans ce salon, vérifiés **à l'émission de
  l'op ET à son rejeu** (avant le durcissement post-revue de D-048,
  `PollCreate` n'exigeait que la simple appartenance au groupe — voir la
  section « Durcissement post-revue » plus haut).
- Clôture (`groups.polls.close`) et suppression (`groups.polls.delete`)
  réservées à l'auteur du sondage ou à un porteur de `MANAGE_CHANNELS`,
  comme l'édition/suppression d'un événement. Une fois clos, tout vote
  ultérieur est ignoré au repli — le dépouillement reste figé. Idempotent
  (clore un sondage déjà clos ne change rien). La suppression, elle, retire
  le sondage de `groups.state.polls` et **libère l'emplacement compté par
  `MAX_POLLS`** — seul moyen de le récupérer une fois un sondage créé.
- Un vote dont `option_index` dépasse le nombre **réel** d'options du
  sondage (connu seulement via le message, jamais de l'op-log) est accepté
  structurellement au repli — borné uniquement à `MAX_POLL_OPTIONS` (10) —
  mais n'est simplement jamais affiché par une UI honnête : dégradation
  gracieuse, pas d'oracle réseau, même politique que la validation d'un nom
  de sticker à l'ingestion.
- Un membre expulsé, banni ou qui quitte volontairement perd son vote sur
  tous les sondages (retiré de l'état, même hygiène que ses RSVP
  d'événement) ; les sondages qu'il a **autorés** restent, clôturables et
  supprimables par le fondateur ou tout `MANAGE_CHANNELS`.

### 6.2 Forme du corps dans l'historique

Forme du corps dans l'historique (`groups.history`, `groups.history_around`,
`event.group_msg`) :

```json
{
  "type": "poll",
  "poll_id": "hex16",
  "question": "Pizza ou sushis ?",
  "options": ["Pizza", "Sushis", "Les deux"]
}
```

### 6.3 Forme dans `groups.state`

Chaque entrée du tableau `polls` (le dépouillement, jamais la question/les
options — cf. `groups.history` ci-dessus) :

```json
{
  "poll_id": "hex16",
  "author": "hex32",
  "channel_id": "hex16",
  "msg_id": "hex16",
  "closed": false,
  "counts": [0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
  "total_votes": 1,
  "my_vote": 1
}
```

- `channel_id`/`msg_id` (champs additifs du durcissement post-revue) :
  reflètent directement `GroupOpBody::PollCreate` — le salon où le sondage a
  été posté et le message canonique (`groups.history`) auquel `poll_id` est
  lié.
- `counts` : tableau **toujours large de `MAX_POLL_OPTIONS` (10)**, quel que
  soit le nombre réel d'options du sondage — `counts[i]` est le nombre de
  votes pour l'option `i` ; les cases au-delà du nombre réel d'options
  restent à `0` en pratique (une UI honnête ne laisse jamais voter hors
  bornes réelles).
- `my_vote` : l'option choisie par l'appelant local, ou `null` s'il n'a pas
  voté.

---

## 7. AutoMod : liste de mots bloqués (`groups.automod.*`)

### 7.1 Modèle honnête P2P — ce que cette op fait et NE fait PAS

**Accord n'a pas de serveur.** Un AutoMod « à la Discord » bloque les
messages *côté serveur*, avant qu'ils n'atteignent qui que ce soit — c'est
impossible à répliquer ici : un client modifié peut toujours signer et
diffuser n'importe quel texte à ses pairs, quel que soit le contenu de
`automod_words`. Prétendre le contraire serait mentir sur la garantie de
sécurité offerte.

Le modèle réaliste, et donc celui implémenté, est :

- La liste de mots bloqués est une **configuration de serveur signée et
  répliquée** par l'op-log de groupe, exactement comme le nom du serveur ou
  ses salons — tous les pairs honnêtes convergent vers la même liste.
- Un **client honnête** est responsable de deux choses, toutes deux hors du
  périmètre de cette vague backend (vague frontend suivante,
  `app/`) :
  1. **À la composition** : avertir (ou bloquer) l'expéditeur *localement*
     avant l'envoi si son message contient un mot de la liste.
  2. **Au rendu** : masquer les mots correspondants dans les messages
     **reçus**, pour se protéger des pairs qui ignorent délibérément
     l'avertissement local (client modifié, ou tout simplement un pair qui
     n'a pas encore synchronisé la liste courante).
- Un pair qui ignore la liste (client modifié, ou ancien client qui ne
  connaît pas encore `SetAutoModWords`) envoie son message normalement ;
  ce backend ne peut ni le refuser ni le censurer a posteriori — il n'y a
  personne au milieu pour le faire. C'est la même limite fondamentale que
  n'importe quelle modération dans un système sans serveur de confiance :
  la seule application *garantie* est celle que chaque pair honnête fait
  sur ses propres réception/composition.

En résumé : ce backend **stocke et réplique la règle** ; il ne
**l'applique** jamais lui-même, faute d'avoir un point de passage obligé
par lequel les messages transiteraient.

### 7.2 Méthodes RPC

| Méthode | Paramètres | Résultat | Erreurs notables |
|---|---|---|---|
| `groups.automod.set` | `{ group_id, words: [string, ...] }` | `{ ok: true }` | `INVALID_PARAMS` (`words` absent/non-liste, plus de 50 entrées, un mot hors bornes) ; `APP_ERROR` « refusé : … » (pas `MANAGE_CHANNELS`) |
| `groups.automod.get` | `{ group_id }` | `{ words: [string, ...] }` | — |

- `groups.automod.set` **remplace intégralement** la liste (comme
  `groups.rename` remplace le nom) — ce n'est jamais un ajout/retrait
  incrémental. Envoyer une liste vide efface le filtre.
- Chaque mot est normalisé en minuscules côté frontière **et** au repli
  (comparaison insensible à la casse) ; des doublons différant seulement
  par la casse (`"Spam"`, `"SPAM"`) fusionnent silencieusement en une seule
  entrée.
- `groups.automod.get` est un raccourci de confort : la même information
  est déjà exposée dans `groups.state.automod_words` (voir §7.3) — inutile
  d'appeler `groups.state` en entier juste pour lire la liste courante.

### 7.3 Forme dans `groups.state`

Nouveau champ racine, tableau de chaînes déjà normalisées (minuscules) et
triées (ordre du `BTreeSet`, déterministe) :

```json
{
  "automod_words": ["scam", "spam"],
  "...": "reste du contrat groups.state inchangé"
}
```

### 7.4 Choix d'implémentation et durcissement adversarial

**Pourquoi `MANAGE_CHANNELS` plutôt qu'un nouveau bit de permission** :
même famille que `SetMeta`/salons/catégories — la liste AutoMod est une
propriété du serveur au même titre que son nom ou ses salons, pas une
ressource à part qui justifierait un bit dédié (contrairement aux émojis/
stickers, qui ont leur propre `MANAGE_EMOJIS` parce qu'un serveur peut
vouloir déléguer *seulement* la gestion des émojis sans donner accès aux
salons/rôles).

**Pourquoi une op de remplacement intégral plutôt que
`AutomodWordAdd`/`AutomodWordRemove`** : une liste de mots bloqués est
consultée comme un tout (« quels mots filtre-t-on en ce moment ? »), jamais
un mot à la fois ; un remplacement intégral est strictement plus simple à
raisonner et à répliquer de façon déterministe (pas de sémantique
d'ensemble à définir pour des ajouts/retraits concurrents), au prix
assumé de devoir renvoyer la liste complète à chaque modification — un coût
négligeable vu la borne de 50 mots.

**Passe adversariale (bornée, sans panique)** :

- Décodage filaire (`accord-proto`) : liste tronquée à tout point rejetée
  intégralement (pas de dépouillement partiel), un mot au-delà de la borne
  filaire (128 octets, marge UTF-8 de 32 caractères) rejeté intégralement,
  octets excédentaires en fin de structure rejetés, fuzz de troncature sur
  tous les préfixes d'un encodage valide.
- Repli (`accord-core`) : permission `MANAGE_CHANNELS` refusée pour un
  simple membre ; plus de 50 mots rejeté (défense en profondeur — le
  décodage filaire l'empêche déjà, le repli ne doit jamais en dépendre
  seul) ; un seul mot invalide (vide, > 32 caractères, caractère de
  contrôle, caractère de format trompeur bidi/zero-width via
  `is_valid_display_label`) **rejette l'op entière** — jamais de
  remplacement partiel avec les mots valides d'une liste par ailleurs
  invalide ; repli prouvé indépendant de l'ordre d'arrivée des ops
  (`fold_is_order_independent`, même méthode que pour les autres ops).
- Aucun nettoyage nécessaire au départ d'un membre : c'est une config de
  serveur, pas un état par membre (contrairement à un pseudo, un avatar ou
  une modération vocale).

---

## 8. Mode lent par salon (`groups.channel.slowmode`)

### 8.1 Investigation préalable — pourquoi ce n'est PAS repliable comme un
`TimeoutMember`

Les MESSAGES de salon (`CoreMsg::GroupMsg`, corps `MsgBody`) ne font **pas**
partie de l'op-log signé et répliqué (`GroupOp`/`GroupOpBody`, replié par
`GroupState::fold`) : ils voyagent comme des livraisons P2P chiffrées
séparées, ingérées individuellement par
`accord_core::group::msg::ingest_group_message`, qui ne consulte
`GroupState` que pour l'autorisation (`VIEW`+`SEND` effectifs, sourdine,
salon d'annonces) — `GroupState::apply` ne voit jamais un message. De plus,
`accord_core::group::group_state` **recalcule** `GroupState::fold(ops)` à
chaque appel (fonction pure de l'op-log) : aucun état mutable dérivé des
messages (comme « dernier envoi de cet auteur ») ne peut donc vivre dans
`GroupState`.

Conséquence directe :
- La **configuration** du mode lent (le cooldown en secondes) EST repliable
  comme n'importe quelle autre propriété de salon — c'est
  `GroupOpBody::SetChannelSlowmode`, stockée dans
  `GroupState::channels[channel_id].slowmode_secs`, gated `MANAGE_CHANNELS`
  à l'émission **et** au rejeu (comme tout op de gestion de salon).
- L'**application** du cooldown (compter le temps écoulé depuis le dernier
  message d'un auteur dans un salon) ne peut PAS être repliée dans
  `GroupState` — il n'existe littéralement aucun mécanisme d'op-log qui voie
  les messages. Elle est donc appliquée par chaque pair HONNÊTE, à la
  composition et à l'ingestion, contre un suivi local hors `GroupState`
  (table `group_slowmode`, non répliquée).

Ce modèle reste néanmoins **robuste contre un client modifié** — bien plus
qu'une simple convention côté envoi — car l'ingestion (pas seulement la
composition) applique la même règle : un message trop rapide envoyé par un
pair hostile est silencieusement ignoré (`GroupMsgEvent::Ignored`) par
**chaque destinataire honnête**, indépendamment de ce que fait l'expéditeur.
C'est exactement le même principe que la sourdine (`TimeoutMember`, 0x1D) :
la permission/config est repliée, l'application au flux de messages est
vérifiée séparément à la composition et à l'ingestion.

### 8.2 Anti-forge : horloge du récepteur, jamais `sent_ms`

`sent_ms` est auto-déclaré par l'expéditeur et non authentifié (hors AAD).
Un client modifié pourrait donc mentir dessus pour prétendre que le cooldown
est écoulé alors que ses messages arrivent en rafale en temps réel. Comme
pour le contournement de sourdine déjà documenté
(`accord_core::group::msg::ingest_group_message`), le suivi du mode lent
utilise **l'horloge locale non falsifiable** :

- à la composition, l'horloge locale de l'émetteur (`now_ms` de l'appelant) ;
- à l'ingestion, l'horloge locale du RÉCEPTEUR (`local_now_ms`), jamais
  `sent_ms`.

Chaque pair honnête rejette donc un message trop rapproché du précédent
message qu'IL a lui-même reçu de cet auteur dans ce salon, quel que soit ce
que l'expéditeur prétend sur `sent_ms`.

### 8.3 Méthode RPC

| Méthode | Paramètres | Résultat | Erreurs notables |
|---|---|---|---|
| `groups.channel.slowmode` | `{ group_id, channel_id, seconds }` | `{ ok: true }` | `INVALID_PARAMS` (`seconds` absent/non entier, > 21600) ; `APP_ERROR` « refusé : … » (pas `MANAGE_CHANNELS`, salon inconnu) |

- `seconds = 0` désactive le mode lent (valeur par défaut de tout salon
  nouvellement créé).
- Bornes : `0..=21600` (6 heures, plafond identique à Discord) — **rejeté au
  décodage filaire** (contrairement à l'échéance d'une sourdine, plafonnée
  silencieusement au repli plutôt que refusée) ; revérifié au repli en
  défense en profondeur.
- Exemptions : un porteur de `MANAGE_CHANNELS` **ou** `MANAGE_MESSAGES`
  dans le salon (overrides compris) n'est jamais bridé par sa propre
  configuration (comportement Discord) — un modérateur peut toujours
  intervenir sans attendre son propre cooldown.

### 8.4 Forme dans `groups.state`

Nouveau champ par salon (0/absent = désactivé) :

```json
{
  "channels": [
    {
      "channel_id": "…",
      "name": "général",
      "slowmode_secs": 30,
      "...": "reste du contrat channel inchangé"
    }
  ]
}
```

### 8.5 Nettoyage à la suppression du salon

`slowmode_secs` vit directement sur la structure `Channel` de `GroupState` :
supprimer le salon (`DelChannel`) retire l'entrée `Channel` tout entière, le
mode lent disparaît donc **gratuitement** avec elle — même mécanique que les
overrides de permissions par salon.

Le suivi LOCAL (non répliqué, table `group_slowmode` : dernier envoi accepté
par (salon, auteur)) est, lui, réélagué après chaque repli de l'op-log
(`accord_core::group::apply_moderation`, même déclencheur que le nettoyage
des tombstones de modération) : toute entrée dont le salon n'existe plus
dans `GroupState.channels`, ou dont l'auteur n'est plus dans
`GroupState.members` (départ, expulsion, bannissement), est purgée — borné
par construction (au plus un couple salon×membre actif à tout instant, pas
de croissance non bornée).

### 8.6 Passe adversariale (bornée, sans panique)

- Décodage filaire : `seconds` au-delà de 21600 rejeté intégralement (pas de
  troncature/plafonnement silencieux) ; fuzz de troncature sur tous les
  préfixes d'un encodage valide ; octets excédentaires en fin de structure
  rejetés.
- Repli (`accord-core`) : permission `MANAGE_CHANNELS` refusée pour un
  simple membre ; salon inconnu ignoré (pas de création d'entrée fantôme) ;
  `seconds` hors bornes ignoré en défense en profondeur (le décodage filaire
  l'empêche déjà, le repli ne doit jamais en dépendre seul) ; repli prouvé
  order-independent.
- Application du cooldown : message trop rapproché ignoré silencieusement
  (`GroupMsgEvent::Ignored`, pas d'erreur — pas d'oracle réseau) ; rôles
  exempts (`MANAGE_CHANNELS`/`MANAGE_MESSAGES`) contournent le cooldown des
  deux côtés (composition et ingestion) ; anti-forge par horloge du
  récepteur (§8.2) prouvée par test (`sent_ms` mensonger loin dans le
  « futur » sans effet sur la décision) ; suivi borné et purgé au départ
  d'un membre ou à la suppression d'un salon (§8.5) ; réingestion d'un
  message déjà accepté (rejeu/doublon réseau) détectée et traitée à part
  (`GroupMsgEvent::Duplicate`) — jamais comptée contre le cooldown de son
  propre auteur.

---

## 9. Quotas et bornes (résumé)

| Élément | Borne | Enforcement |
|---|---|---|
| Événements par groupe | 25 (`MAX_EVENTS`) | fold, à la création uniquement (édition/suppression/RSVP toujours possibles au-delà) |
| Titre d'événement | 2-100 caractères, sans caractère de contrôle ni de format trompeur (bidi/zero-width) | frontière + fold |
| Description d'événement | ≤ 1024 caractères, caractères de contrôle refusés hors `\n`/`\r`/`\t` | frontière + fold |
| Échéance d'événement (`start_ms`) | ≤ 1<<43 ms (~an 2248, même plafond qu'un timeout) | fold — **rejet complet de l'op**, jamais de troncature silencieuse (contrairement à un timeout) |
| Salon d'un événement | doit être un salon **vocal** existant du groupe, sinon ignoré | fold |
| Stickers par groupe | 30 (`MAX_STICKERS`) | fold, sur un **nouveau** nom uniquement (remplacer un nom existant reste toujours permis) |
| Nom de sticker | 2-32 caractères `[a-z0-9_]` | frontière + fold |
| Image de sticker | 1 octet – 512 Kio décodée, MIME `image/{png,jpeg,webp,gif}` | frontière (**convention côté client** : appliquée quand le nœud LOCAL crée l'op — au rejeu, seul le hash 32 octets est validé ; un client modifié peut référencer un blob plus gros, comme pour les émojis/icônes existants. Le fetch reste explicite et borné par la couche fichiers) |
| Image d'avatar de serveur | 1 octet – 512 Kio décodée, MIME `image/*` | frontière (même convention côté client que les stickers) |
| Couleur de bannière / de rôle | ≤ 0xFFFFFF (24 bits) | frontière + décodage filaire |
| RSVP | dédoublonné par `(event_id, membre)`, borné par l'appartenance au groupe | fold |
| Nom de sticker sur le fil (`MsgBody::Sticker`) | ≤ 32 octets (`MAX_EMOJI_NAME`, partagé avec les émojis) | décodage filaire |
| Titre d'événement sur le fil | ≤ 400 octets UTF-8 | décodage filaire |
| Description d'événement sur le fil | ≤ 4096 octets UTF-8 | décodage filaire |
| Sondages par groupe | 25 (`MAX_POLLS`) | fold, à la création (`PollCreate`) uniquement — récupérable via `PollDelete` (vote/clôture toujours possibles au-delà du plafond) |
| Création de sondage (`PollCreate`) | `VIEW`+`SEND` effectifs de l'auteur dans `channel_id` (**pas** la simple appartenance) | fold, même porte qu'un envoi de message ordinaire |
| Question de sondage | 1-300 octets UTF-8, non vide, sans caractère de contrôle hors `\n`/`\r`/`\t` | décodage filaire (bornes d'octets/vide/UTF-8) + composition/ingestion (caractères de contrôle) |
| Options de sondage | 2-10 entrées, chacune 1-100 octets UTF-8, non vide | décodage filaire + composition/ingestion |
| `option_index` d'un vote | borne **structurelle** ≤ `MAX_POLL_OPTIONS` (10) — le repli ignore ce qui dépasse, indépendamment du nombre réel d'options du sondage (connu seulement via le message) | fold |
| Vote de sondage | dédoublonné par `(poll_id, membre)`, choix unique (remplace, n'accumule pas) | fold |
| Liaison `poll_id` → message canonique | un seul `(auteur, msg_id)` par `poll_id`, fixé par le premier `PollCreate` replié — tout `MsgBody::Poll` ultérieur d'un autre auteur ou avec un autre `msg_id` est ignoré | ingestion du message (contre l'état replié de l'op-log) |
| Mots AutoMod par groupe | 50 (`MAX_AUTOMOD_WORDS`) | décodage filaire **et** fold (défense en profondeur) — `SetAutoModWords` remplace toujours la liste entière, jamais d'accumulation |
| Mot AutoMod | 1-32 caractères après normalisation minuscule, sans caractère de contrôle ni de format trompeur (bidi/zero-width) | frontière (bornes structurelles) + fold (`is_valid_display_label`, rejet complet de l'op sur un seul mot invalide) |
| Mot AutoMod sur le fil (`SetAutoModWords`) | ≤ 128 octets UTF-8 (marge ×4 sur 32 caractères, même politique que `MAX_NICKNAME`) | décodage filaire |
| `SetAutoModWords` | `MANAGE_CHANNELS` requis | fold, à l'émission **et** au rejeu |
| Mode lent par salon (`slowmode_secs`) | 0-21600 secondes (6h, `MAX_CHANNEL_SLOWMODE_SECS`) | **décodage filaire** (rejet complet au-delà, pas de troncature) **et** fold (défense en profondeur) |
| `SetChannelSlowmode` | `MANAGE_CHANNELS` requis, salon existant | fold, à l'émission **et** au rejeu |
| Application du cooldown de mode lent | non repliable (les messages ne font pas partie de l'op-log — voir §8.1) : chaque pair honnête l'applique localement, horodaté par SA PROPRE horloge (jamais `sent_ms` auto-déclaré) | composition **et** ingestion (`accord_core::group::msg::check_slowmode`) |
| Exemption de mode lent | `MANAGE_CHANNELS` ou `MANAGE_MESSAGES` effectif dans le salon | composition **et** ingestion |
| Suivi local du mode lent (table `group_slowmode`) | un couple (salon, auteur) actif à la fois, purgé au départ d'un membre ou à la suppression d'un salon | réélagage après chaque repli de l'op-log |

Toute op au-delà d'une borne de **fold** est silencieusement ignorée
(convergence déterministe entre pairs honnêtes, indépendante de l'ordre
d'arrivée — cf. `GroupState::apply`) ; toute valeur au-delà d'une borne de
**décodage filaire** rejette la structure entière avant même d'atteindre le
repli (aucun panic possible sur une entrée hostile).
