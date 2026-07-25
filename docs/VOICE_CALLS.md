# Suite vocale : appels 1-à-1, DSP de capture, modération vocale

Contrat API (JSON-RPC + événements WebSocket) de la suite vocale introduite
par `feature/voice-suite`. Tout est **additif** : aucun contrat existant
(`voice.*` D-025/D-029, `groups.*`) n'est modifié, seulement étendu.

Les identifiants (`peer`, `call_id`, `group_id`, …) transitent en
**hexadécimal minuscule** (32 caractères pour 16 octets, 64 pour une clé
publique), comme partout ailleurs dans l'API.

---

## 1. Appels 1-à-1 (`calls.*`)

Un seul appel à la fois par nœud (toute phase confondue). L'appelé doit être
un **ami confirmé**. Une fois l'appel accepté, la session audio réutilise le
moteur vocal existant : mêmes RPC `voice.mute` / `voice.deafen` /
`voice.set_volume` / `voice.status`, mêmes événements `event.voice_speaking`
/ `event.voice_mute`, mêmes réglages DSP.

### 1.1 Méthodes RPC

| Méthode | Paramètres | Résultat | Erreurs |
|---|---|---|---|
| `calls.start` | `{ "peer": hex32 }` | `{ "call_id": hex16 }` | non-ami ; soi-même ; « appel déjà en cours » (occupé) |
| `calls.accept` | `{ "call_id": hex16 }` | `{ "ok": true }` | aucun appel entrant ; `call_id` inconnu |
| `calls.decline` | `{ "call_id": hex16 }` | `{ "ok": true }` | aucun appel entrant ; `call_id` inconnu |
| `calls.hangup` | `{}` | `{ "ok": true }` | — (idempotent au repos) |
| `calls.status` | `{}` | voir ci-dessous | — |

`calls.status` rend :

```json
{
  "state": "idle" | "outgoing_ringing" | "incoming_ringing" | "active",
  "peer": "hex32 ou null",
  "call_id": "hex16 ou null",
  "since_ms": 12345
}
```

`since_ms` est le début de la phase courante sur l'horloge **interne du
moteur** (millisecondes depuis son démarrage) — à utiliser pour des durées
relatives, pas comme un temps mural. `null` au repos.

`calls.hangup` couvre les trois phases : il **annule** une sonnerie sortante,
**refuse** une sonnerie entrante et **raccroche** un appel actif.

### 1.2 Événements

| Événement | Champs JSON | Quand |
|---|---|---|
| `event.call_outgoing` | `{ "peer", "call_id" }` | notre offre part (après `calls.start`) |
| `event.call_incoming` | `{ "peer", "call_id" }` | l'offre d'un AMI sonne chez nous |
| `event.call_accepted` | `{ "peer", "call_id" }` | l'appel devient actif (émis des deux côtés) |
| `event.call_ended` | `{ "peer", "call_id", "reason" }` | l'appel se termine, quelle que soit la phase |

`reason` de `event.call_ended` (chaînes stables) :

| `reason` | Sens |
|---|---|
| `"hangup"` | raccrochage (local ou distant) d'un appel actif, ou annulation de notre sonnerie sortante |
| `"declined"` | l'appelé a refusé (ou nous avons refusé localement) |
| `"busy"` | l'appelé est déjà en appel |
| `"timeout"` | notre sonnerie sortante a expiré (45 s) sans réponse |
| `"missed"` | la sonnerie entrante a expiré (45 s) sans que l'on réponde — « appel manqué » |
| `"canceled"` | la sonnerie entrante s'arrête avant son échéance : l'appelant a annulé, **ou** ses réémissions d'offre se sont taries (§1.5) |
| `"answered_elsewhere"` | un autre appareil du même compte a décroché ; la sonnerie s'arrête et ce n'est **pas** un appel manqué (§1.5) |
| `"lost"` | la liaison audio avec le pair a été perdue en plein appel (~10 s de silence réseau) |
| `"superseded"` | appels croisés : notre appel sortant est remplacé par celui du pair (immédiatement suivi de `event.call_accepted` sur l'appel retenu) |

Pendant un appel actif, les événements vocaux habituels s'appliquent :
`event.voice_speaking { pubkey, speaking }`, `event.voice_mute { pubkey,
muted, deafened }`, et `event.voice_joined` / `event.voice_left` avec
`group_id = "000…0"` (sentinelle, 32 zéros) et `channel_id = call_id`.

### 1.3 États et transitions

```
                    calls.start
        idle ────────────────────────► outgoing_ringing
         ▲                                   │
         │  event.call_ended                 │ CallAnswer reçu
         │  (declined/busy/timeout/          ▼
         │   hangup/superseded)           active ◄──────────────┐
         │                                   │                  │
         │                                   │ hangup local     │ calls.accept
         │                                   │ ou distant,      │
         │◄──────────────────────────────────┘ perte (lost),    │
         │                                     voice.join       │
         │  offre d'un ami                                      │
        idle ────────────────────────► incoming_ringing ────────┘
                event.call_incoming          │
                                             │ calls.decline / expiration (missed)
                                             │ / CallHangup de l'appelant (canceled)
                                             │ / plus d'offre reçue (canceled, §1.5)
                                             │ / CallTaken (answered_elsewhere, §1.5)
                                             ▼
                                           idle
```

Règles complémentaires :

- **Occupé** : toute offre reçue alors qu'un appel est en cours (n'importe
  quelle phase, autre pair) déclenche un refus automatique `busy` chez
  l'appelant. `calls.start` pendant un appel rend une erreur.
- **Sonnerie et perte de paquets** : l'appelant réémet son offre toutes les
  2 s pendant la sonnerie, et **cesse net dès qu'il décroche** ; l'appelé
  déduplique par `call_id` (une seule sonnerie, l'échéance n'est jamais
  prolongée par les réémissions). L'arrêt des réémissions n'est pas un détail
  d'optimisation : c'est le signal sur lequel repose l'extinction des autres
  appareils (§1.5).
- **Timeout de sonnerie** : 45 s des deux côtés.
- **Appels croisés** (chacun appelle l'autre en même temps) : convergence
  déterministe vers l'appel de la plus petite clé publique ; le perdant émet
  `event.call_ended { reason: "superseded" }` puis `event.call_accepted` —
  les deux utilisateurs se retrouvent dans le même appel sans intervention.
- **`voice.join` pendant un appel actif** : l'appel est raccroché (le salon
  de groupe prend la session audio). Une simple sonnerie survit à
  `voice.join`.
- **`voice.leave` pendant un appel actif** : équivaut à `calls.hangup`.

### 1.4 `voice.status` pendant un appel

`voice.status.active` est non-nul pendant un appel actif, avec :

```json
{
  "active": {
    "group_id": "00000000000000000000000000000000",
    "channel_id": "<call_id>",
    "is_call": true,
    "muted": false,
    "deafened": false,
    "participants": [ { …, voir §3.3 } ]
  },
  "master_volume": 100,
  "dsp": { "noise_suppression": false, "agc": false }
}
```

`is_call` (booléen, additif) distingue une session d'appel d'un salon de
groupe. Champ présent aussi pour les salons de groupe (`false`).

### 1.5 Multi-appareil : tous les appareils sonnent, un seul décroche

Un `CallOffer` est adressé au **compte** de l'appelé, et la couche de livraison
l'éclate sur chacun de ses appareils joignables (`MULTI_DEVICE.md` §3.2.1,
§5.1). Tous sonnent donc, et il faut éteindre les autres dès que l'un décroche.
Deux mécanismes s'en chargent, et **un seul des deux garantit la correction**.

**1. `CallTaken` (0x1A) — le raccourci de latence.** Dès que l'appelant honore
une réponse, il émet `CallTaken` vers le **compte** de l'appelé : trois
émissions espacées de 2 s, pour absorber quelques pertes. Le message part donc
vers *tous* les appareils de l'appelé, **y compris le gagnant**, qui l'ignore
parce qu'il ne sonne plus. Chez un appareil qui sonne encore et qui corrèle
exactement (même pair, même `call_id`), il termine la sonnerie avec
`reason: "answered_elsewhere"`.

**2. Le tarissement des offres — le filet.** Il ne dépend d'**aucun** message
reçu. L'appelant réémet son offre toutes les 2 s tant que ça sonne et cesse net
dès qu'il décroche ; un appareil qui sonne sans plus rien recevoir pendant 8 s
(quatre périodes de réémission) en conclut de lui-même et termine avec
`reason: "canceled"`.

L'ordre entre les deux est délibéré. `CallTaken` voyage en UDP et n'est jamais
mis en file hors-ligne — livré une heure plus tard, il éteindrait une sonnerie
sans rapport — donc les trois datagrammes peuvent se perdre en entier. Si la
correction en dépendait, les autres appareils sonneraient les 45 s complètes
puis afficheraient un **appel manqué** pour un appel qui a été pris : une
notification fausse, qui est pire qu'une sonnerie qui s'arrête quelques secondes
trop tard. `CallTaken` n'achète donc que de la latence — sans lui, la sonnerie
résiduelle dure jusqu'à 8 s au lieu d'un aller-retour.

Le filet ne doit pas casser l'invariant de la sonnerie : **une offre rejouée
n'allonge jamais l'échéance des 45 s**. Deux dates sont suivies séparément —
celle de la première offre, qui fixe l'échéance et ne bouge pas, et celle de la
dernière offre reçue, qui seule alimente le filet.

**L'annonce est à sens unique.** L'appareil qui décroche n'émet rien : seul
l'appelant sait quelle réponse il a honorée, et une annonce émise par l'appelé
partirait vers le compte de l'**appelant**, dont les appareils ne sonnent pas —
du bruit sans effet, et la fausse impression que le cas est couvert.

**Limite connue : deux appareils qui décrochent dans le même aller-retour.**
L'appelant ignore la seconde réponse (sa machine n'est plus en sonnerie
sortante), mais il ne peut pas *désigner* le perdant : la signalisation d'appel
est traduite en compte à l'entrée du routeur, si bien que les deux réponses
portent exactement la même clé. Le perdant reste donc dans un appel actif dont
il ne reçoit aucun média et n'en sort qu'au bout de ~10 s, par la détection de
perte audio (`reason: "lost"`).

🔒 Cette sortie était bien pire que gênante : la perte audio émettait un
raccrochage best-effort, qui corrélait chez l'appelant exactement comme celui du
gagnant et **interrompait l'appel réellement établi**. La perte audio est
désormais **silencieuse**. Le message ne servait que là où il était inutile — on
perd l'audio précisément quand le pair a disparu, donc le raccrochage
n'atteignait déjà personne — et nuisait dans le seul cas où il arrivait. Il ne
reste de la course que dix secondes d'un second appareil affichant un appel
qu'il n'entend pas : un défaut d'affichage, plus une perte d'appel.

Fermer la course entièrement demande de faire remonter la **clé de transport**
jusqu'au moteur voix, ce qui n'est pas fait — et réglerait du même coup les
tampons de gigue, qui mêlent aujourd'hui les trames de deux appareils dans une
seule suite de numéros de séquence.

---

## 2. Qualité audio : suppression de bruit et AGC

Deux étages DSP sur la **capture locale**, appliqués avant la détection
d'activité vocale et l'encodage : suppression de bruit (RNNoise, crate Rust
pure `nnnoiseless`) puis contrôle automatique de gain (cible −26 dBFS,
montée lente / descente rapide, gain borné ±12 dB).

Réglages **persistés** (par profil), appliqués à chaud à la session active
(salon comme appel). **Défaut : désactivés** — l'UI choisit sa politique
(activer la suppression de bruit par défaut est raisonnable, parité
Discord).

| Méthode | Paramètres | Résultat |
|---|---|---|
| `voice.set_noise_suppression` | `{ "enabled": bool }` | `{}` |
| `voice.set_agc` | `{ "enabled": bool }` | `{}` |

L'état courant est exposé par `voice.status` (champ additif `dsp`) :

```json
"dsp": { "noise_suppression": true, "agc": false }
```

**Coût CPU mesuré** (Apple M-series, build release, trame mono 20 ms à
48 kHz) : ≈ 105 µs/trame pour la chaîne complète (RNNoise domine, l'AGC est
négligeable), soit **≈ 0,5 % d'un cœur**. Mesure reproductible :
`cargo test -p accord-voice --release -- --ignored --nocapture`.

---

## 3. Modération vocale serveur (op de groupe 0x1F)

Un modérateur force la sourdine (`mute`) et/ou la surdité (`deafen`) d'un
membre dans **tous les salons vocaux du groupe**. Op répliquée dans l'op-log
signé, permission vérifiée au rejeu **et** à l'émission comme
`TimeoutMember` (0x1D) : permission `KICK`, hiérarchie de rôles (cible
strictement en dessous), fondateur intouchable. Un kick/ban/départ efface la
modération du membre.

Application par le moteur vocal de chaque pair honnête :

- la **cible** cesse d'émettre (capture coupée à la source) et/ou cesse de
  décoder (sortie coupée) ;
- les **auditeurs** jettent les trames d'un membre server-muted à la
  réception (défense en profondeur contre un client modifié) ;
- `mute` et `deafen` sont indépendants (un `deafen` seul n'empêche pas de
  parler — sémantique Discord).

### 3.1 Méthode RPC

| Méthode | Paramètres | Résultat | Erreurs |
|---|---|---|---|
| `groups.voice_moderate` | `{ "group_id": hex16, "pubkey": hex32, "mute": bool, "deafen": bool }` | `{ "ok": true }` | `refusé : …` (permission/hiérarchie), cible non membre |

`mute` et `deafen` absents valent `false` ; `{ mute: false, deafen: false }`
**lève** la modération du membre.

### 3.2 Événements et état

- `event.group_op` / `event.group_state { group_id }` : émis comme pour
  toute op (l'UI recharge `groups.state`).
- `groups.state.members[]` porte deux champs additifs :

```json
{ "pubkey": "…", …, "voice_muted": true, "voice_deafened": false }
```

- `event.voice_moderate` : émis par le moteur vocal quand la modération d'un
  participant **du salon actif** change (y compris la levée) :

```json
{
  "group_id": "hex16",
  "pubkey": "hex32",
  "server_muted": true,
  "server_deafened": false,
  "priority_speaker": false
}
```

### 3.3 Participants de `voice.status`

Chaque participant porte trois champs additifs :

```json
{
  "pubkey": "hex32",
  "speaking": false,
  "muted": false,
  "deafened": false,
  "volume": 100,
  "server_muted": false,
  "server_deafened": false,
  "priority_speaker": false
}
```

(`server_*` et `priority_speaker` sont toujours `false` dans une session
d'appel 1-à-1 : la modération est un concept de groupe.)

---

## 4. Priority speaker (bonus)

Nouvelle permission de rôle `PRIORITY_SPEAKER = 1024` (bitfield existant).
Pendant qu'un porteur de cette permission **parle** dans un salon vocal, la
sortie des autres participants est atténuée localement (×0,3 ≈ −10 dB) chez
chaque auditeur ; l'atténuation se relâche dès qu'il se tait (hystérésis
« parle » de 400 ms).

**Volontairement non impliquée par `ADMIN` ni par le statut de fondateur** :
sinon tout fondateur atténuerait son salon en permanence. Elle s'attribue
explicitement via un rôle (`groups.role.add` / `groups.role.edit` avec le
bit 1024).

Exposée par `voice.status` (`priority_speaker` par participant) et par
`event.voice_moderate`.

---

## 5. Wire (information — non exposé à l'UI)

Nouveaux messages CORE additifs, éphémères (jamais mis en file hors-ligne),
transportés dans les sessions chiffrées authentifiées existantes :

```
0x11 CALL_OFFER   { call_id: bytes<16> }
0x12 CALL_ANSWER  { call_id: bytes<16> }
0x13 CALL_DECLINE { call_id: bytes<16>, reason: u8 (0=refusé, 1=occupé) }
0x14 CALL_HANGUP  { call_id: bytes<16> }
0x1A CALL_TAKEN   { call_id: bytes<16> }
```

`CALL_TAKEN` (§1.5) est émis par l'**appelant** vers le **compte** de l'appelé
quand il honore une réponse ; les opcodes 0x15–0x19 appartiennent aux
invitations et à l'appairage.

Nouvelle op de groupe :

```
0x1F VOICE_MODERATE { member: bytes<32>, mute: u8(0|1), deafen: u8(0|1) }
```

Les trames audio d'un appel circulent sur le canal VOICE existant avec
`room == call_id`.

Garde-fous (P2P public, chaque octet est contrôlé par un attaquant) :

- décodage strictement borné, booléens stricts (0|1), `reason` borné,
  octets excédentaires rejetés — jamais de panique sur entrée forgée ;
- une offre n'est honorée que d'un **ami confirmé** ; un non-ami ne
  déclenche **aucune** réponse (zéro amplification) ;
- cadence par pair : au plus une nouvelle sonnerie / 3 s et une réponse
  « occupé » / 2 s par pair, suivi par pair borné (256 entrées) ;
- anti-rejeu : `CallAnswer`/`CallDecline`/`CallHangup`/`CallTaken` ne sont
  honorés que s'ils corrèlent exactement le pair ET le `call_id` de l'appel
  courant ; une offre rejouée ne prolonge jamais une sonnerie ;
- `CallTaken` n'agit **que** sur une sonnerie entrante, jamais sur un appel
  actif : forgé ou rejoué, il ne peut au pire qu'éteindre une sonnerie que son
  propre appelant a déjà quittée — ce qu'un `CallHangup` du même pair ferait de
  toute façon ;
- les trames VOICE d'un pair hors du salon actif (ou d'un membre
  server-muted) sont jetées.
