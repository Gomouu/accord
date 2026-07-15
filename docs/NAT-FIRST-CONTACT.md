# Premier contact entrant sans ouverture de port — diagnostic et correctif

Symptôme rapporté : pour recevoir une **première** invitation de serveur (ou un
premier contact entrant quelconque), l'utilisateur devait ouvrir un port de sa
box au moins une fois. Ensuite le port pouvait être refermé et tout continuait
de fonctionner.

## 1. Diagnostic

Le chemin nominal du premier contact (SPEC §11.3, `docs/NAT_TRAVERSAL.md`) est
le rendez-vous « relais domicile » : B entretient des sessions sortantes vers
les `HOME_RELAY_COUNT` nœuds-relais les plus proches (XOR) de son identifiant ;
A, qui ne connaît que la clé publique de B, recalcule la même dérivation et
ouvre un circuit relais. La conception est correcte — mais **quatre maillons
étaient cassés en production**, tous masqués par les tests qui ensemencent la
DHT hors-bande (`dht_bootstrap(vec![node_info()])`).

### D1 — Les tables de routage DHT n'apprennent jamais personne (cause racine)

Un `NodeInfo` n'entre dans une table de routage que via `observe()` : graines
hors-bande (tests uniquement) ou gossip `FOUND_NODES` (qui ne fait que
propager des entrées déjà apprises ailleurs). Le seul point d'apprentissage
« organique » — l'insertion de l'émetteur d'un RPC entrant
(`KademliaNode::handle_rpc`) — était mort : le runtime synthétise l'émetteur
avec `pow_nonce: 0, flags: 0` (`Runtime::route_dht`), donc `valid_node()`
rejette systématiquement (PoW invalide).

Conséquences en chaîne dans un déploiement réel (amorçage par adresse seule) :

- les tables de routage restent vides ⇒ `home_relays_of()` et
  `select_relay_for()` ne rendent **aucun candidat** ⇒ `ensure_relay_to()` est
  un no-op ⇒ le rendez-vous relais du premier contact n'existe pas ;
- les `put` DHT (présence, identité) ne se répliquent presque nulle part ;
- le seul chemin entrant qui marche est le direct : port ouvert (manuel ou
  UPnP). D'où le symptôme exact : ouvrir un port une fois permet la session
  directe ; ensuite les keep-alives (25 s ≪ timeout de mapping NAT)
  maintiennent le trou, et le port peut être refermé.

### D2 — Repli relais conditionné à la résolution de présence

Dans `presence_resolve_tick`, le repli relais (et le poinçonnage coordonné)
n'étaient déclenchés **que si** le record de présence du pair se résolvait avec
au moins une adresse. Un pair derrière un NAT symétrique peut n'avoir aucun
record résoluble (rien de répliqué, ou pas encore publié) : la boucle passait
au suivant sans jamais tenter le relais — alors que le rendez-vous domicile
n'a **pas besoin** de la présence (il ne dépend que de la clé publique).

### D3 — Sélection des relais sur la seule vue locale

`home_relays_of()` filtrait `closest_local()` : avec une table clairsemée ou
divergente, A et B ne convergent pas vers les mêmes relais (le rendez-vous
exige que les deux dérivent le même ensemble). Il manquait un
`lookup_node(node_id(B))` réseau pour aligner la vue locale sur la vue globale
avant sélection.

### D4 — Livraison de la file d'attente aveugle au circuit

Une session relayée a `addr = adresse du relais` mais n'est pas indexée par
adresse. `outbox_tick`, `flush_peer`, `profile_tick` et `group_sync_tick`
envoyaient via `endpoint.send_to(addr_of(pair), …)` seulement : sur une session
relayée, cela tombe sur la session directe avec le **relais** ⇒
`PeerIdentityMismatch` ⇒ l'invitation/demande d'ami en file n'était **jamais**
livrée dans le circuit (seul `deliver_core` avait le repli circuit).

### D5 — Trou noir de l'outbox via le handshake spéculatif

`Endpoint::send_to` vers une adresse SANS session met le message en file d'un
handshake spéculatif et rend `Ok` (comportement voulu pour la première
émission). Les boucles de maintenance prenaient ce `Ok` pour une livraison et
retiraient l'élément d'outbox — alors que le HELLO partait vers une adresse
de présence injoignable (pair NATé) : la demande d'ami/l'invitation était
définitivement perdue avant même que le circuit relais n'existe.

### Pistes examinées et hors de cause

- *Poinçonnage poule/œuf* : réel (le poinçonnage coordonné exige un lien
  existant) mais déjà traité par conception — le circuit relais est le canal de
  signalisation. C'est le circuit qui manquait (D1–D3).
- *NAT symétrique* : casse bien le poinçonnage direct (mapping par
  destination), mais le relais est précisément le repli prévu ; il ne
  fonctionnait pas pour les raisons ci-dessus.
- *Mailbox* : la relève des boîtes aux lettres DHT ne couvre que les **amis**
  (la clé de boîte dépend de l'émetteur, irrésolvable pour un inconnu). Limite
  réelle mais orthogonale : elle ne concerne que le premier contact **hors
  ligne** (voir « limites » ci-dessous).

## 2. Conception retenue

Approches évaluées :

1. **UPnP/NAT-PMP comme chemin principal** — écartée : souvent désactivé sur
   les box ; reste le bonus opportuniste existant (`node/nat.rs`), jamais le
   chemin critique.
2. **Boîte aux lettres « premier contact »** (clé dérivée du seul destinataire,
   relevable sans connaître l'émetteur) — écartée comme chemin principal :
   surface de spam/DoS non authentifiée (n'importe qui peut déposer), ne donne
   pas d'interactivité (pas de handshake tunnelé), et ne résout pas la
   signalisation du poinçonnage. Reste une extension possible pour le premier
   contact *hors ligne*.
3. **Réparer le rendez-vous relais domicile existant** (esprit libp2p Circuit
   Relay v2 pour la réservation = sessions entretenues, + DCUtR pour l'upgrade
   punch coordonné, déjà implémenté) — **retenue** : l'architecture était déjà
   conçue pour cela ; les défauts étaient des trous de découverte/livraison
   (D1–D4), pas de conception.

### Correctifs

- **`NODE_ANNOUNCE` (CONTROL 0x08, additif)** : à l'établissement de chaque
  session **directe**, l'initiateur annonce `{pow_nonce, flags}` ; le récepteur
  vérifie (PoW + cohérence `node_id`/clé authentifiée de session), construit un
  `NodeInfo` dont l'adresse est **l'adresse source observée** (jamais une
  adresse déclarée — pas de redirection de tiers) et l'insère dans sa table de
  routage. Le récepteur répond une fois par session avec sa propre annonce
  (poignée de main d'annonce, insensible à l'ordre WELCOME/DATA). Un ancien
  pair rejette le message au décodage (`MALFORMED` silencieux, SPEC §12) :
  compatibilité filaire additive, même schéma que 0x06/0x07.
- **Éligibilité relais** (`relay_eligible`, fonction pure) : le drapeau RELAY
  n'est plus annoncé inconditionnellement mais seulement si le nœud est
  plausiblement joignable de l'extérieur — mapping UPnP/NAT-PMP actif, **ou**
  consensus d'adresse observée dont le port égale le port local (nœud public,
  redirection existante, ou NAT préservant le port). Sans ce gating, chaque
  nœud NATé se serait annoncé relais dès que l'annonce fonctionne, et les
  « relais domicile » élus auraient été injoignables — cassant le rendez-vous.
  Ré-annonce aux sessions directes quand l'éligibilité change.
- **Repli inconditionnel** (`presence_resolve_tick`) : pour chaque cible, le
  repli (relais après `PUNCH_FALLBACK_MS`, puis demande de poinçonnage
  coordonné entre amis) est déclenché **même sans record de présence**.
- **Convergence de sélection** : `ensure_relay_to` fait un `lookup_node`
  réseau de l'identifiant du pair avant la sélection ; `home_relay_tick` fait
  de même sur son propre identifiant. A et B dérivent ainsi leurs relais de la
  même vue globale.
- **Livraison via le meilleur lien ÉTABLI** (`Runtime::send_via_best_link`) :
  `outbox_tick`, `flush_peer`, `profile_tick` et `group_sync_tick` n'émettent
  que sur une session directe établie (adresse tenue par l'endpoint,
  `direct_session_addr`) ou un circuit relais existant — jamais via le
  handshake spéculatif (D5) : sans lien, échec franc et le message RESTE en
  file, relivré par `flush_peer` à l'établissement du lien.
- **Cache d'annonces** : le runtime mémorise `(pow_nonce, flags)` annoncés par
  pair et les réutilise pour reconstruire le `NodeInfo` d'un émetteur de RPC
  entrant — sans quoi l'insertion `handle_rpc` (valeurs synthétisées à zéro)
  écraserait le drapeau relais d'une entrée à chaque RPC (observé à PoW
  faible ; en production la PoW invalide masquait ce chemin).

### Sécurité

- L'annonce n'est acceptée **que dans une session chiffrée authentifiée**
  (identité prouvée par le handshake, PoW re-vérifié par `valid_node`), et
  **jamais** sur une session tunnelée (sinon l'adresse du relais empoisonnerait
  la table). L'adresse insérée est l'adresse source observée : un pair ne peut
  pas faire pointer une entrée vers une victime tierce. Coût borné : une
  insertion de table (LRS + diversité /24 déjà en place) par annonce ACCEPTÉE,
  réponse au plus une par session, ET traitement plafonné par le seau de
  contrôle de session (H1, voir §3bis) — un pair authentifié ne peut plus
  inonder.
- Le relais reste aveugle (blobs DATA d'une session bout-en-bout, liaison
  d'identité D-037 aux deux extrémités) et borné (64 circuits, 1 Mo/s,
  session avec la cible exigée à l'ouverture). Rien de nouveau côté relais.
- Le gossip `FOUND_NODES` peut toujours propager des `NodeInfo` forgés vers
  des adresses arbitraires (préexistant, borné par `RELAY_TRY_MAX` et le coût
  d'un handshake) — inchangé, documenté dans `docs/THREAT-MODEL.md` §2.
- Décodage strict de la nouvelle surface : `NODE_ANNOUNCE` est de taille fixe
  (9 octets utiles), aucune allocation pilotée par l'attaquant.

## 3. Preuve reproductible

- `accord-transport::socket::sim` gagne un **NAT symétrique simulé** : mapping
  externe par destination (nouveau port par destination), filtrage entrant
  strict (seule la destination d'origine peut répondre sur un mapping), aucun
  entrant non sollicité vers l'adresse interne. Sémantique testée unitairement.
- `accord-node` expose `run_with_socket` (injection du socket datagramme) : le
  nœud **complet** (DHT, maintenance, outbox, relais) tourne sur le mesh
  simulé.
- `crates/accord-node/tests/nat_first_contact_e2e.rs` : R public (relais
  potentiel), A et B derrière deux NAT symétriques distincts, **aucun entrant
  non sollicité possible**. A n'a que la clé publique de B. Le test vérifie la
  demande d'ami A→B livrée, l'amitié confirmée dans les deux sens et un DM
  aller-retour — sans aucune ouverture de port. Ce test échoue sur l'arbre
  antérieur au correctif (vérifié : timeout, la demande n'atteint jamais B) et
  passe après.

## 3bis. Durcissement post-revue de sécurité (H1, M1)

Une revue adverse a validé le cœur (auth, décodage, PoW, rejet-tunnel,
confidentialité relais, no-drop) et relevé trois points corrigés avant release :

- **H1 (débit `NODE_ANNOUNCE`)** : traité dans la couche transport, il
  contournait les token-buckets DHT — un pair authentifié pouvait inonder des
  annonces à plein débit UDP (contention de verrous, croissance du canal
  d'événements non borné). Correctif : un **seau à jetons par session**
  (`Session::ctrl_bucket`, rafale 8, recharge 1/s) borne le traitement des
  messages de contrôle changeant l'état (`NODE_ANNOUNCE`, `OBSERVE_ADDR_RESP`)
  AVANT toute remontée d'événement. Test : `handshake_e2e::
  node_announce_flood_borne_par_session` (200 annonces émises, ≤ 8 acceptées).
- **M1a (drapeau RELAY falsifiable)** : le flag est auto-déclaré et gratuit. Le
  récepteur ne s'y fie plus seul : un relais n'est prioritaire dans la sélection
  qu'après une **preuve de joignabilité active** — flag RELAY annoncé DANS une
  session directe établie, suivi dans `verified_relays` et remonté en tête par
  `relay::prioritize_reachable` avant le bornage `RELAY_TRY_MAX`. Un flot de
  faux relais injoignables n'évince donc plus le relais réel. Résiduel assumé
  (relais joignable mais malveillant) : cf. `docs/THREAT-MODEL.md` §2. Tests :
  `relay::priorisation_relais_verifies_survit_au_flot_de_faux`.
- **M1b (consensus d'observation forgeable)** : `ObservedAddrs` comptait les
  votes PAR MESSAGE ; un pair unique fabriquait un « consensus » (2 votes) et
  faisait basculer `relay_eligible` d'une victime NATée. Correctif : votes
  **dédupliqués par identité d'observateur** (dernier vote par pair), consensus
  exigeant ≥ 2 pairs DISTINCTS, et `OBSERVE_ADDR_RESP` throttlé par le seau de
  contrôle de session + restreint aux liens directs. Tests : `nat::
  un_seul_pair_ne_fabrique_pas_de_consensus`, `relay::
  un_pair_seul_ne_rend_pas_une_victime_natee_eligible`.
- **M2 (footgun)** : `run_with_socket` (injection d'un socket non durci) est
  `#[doc(hidden)]` et documenté réservé aux tests ; `run_node` reste privé.

## 3ter. « Code ami introuvable sur le réseau » — rendez-vous partagé (défaut)

Symptôme rapporté en production : à l'invitation, l'inviteur voit « Code ami
introuvable sur le réseau » ; ouvrir un port chez l'invité corrige.

**Diagnostic.** C'est un chemin DISTINCT du premier-contact ci-dessus : le VRAI
premier pas d'une invitation est la RÉSOLUTION du code ami — un `FIND_VALUE` DHT
du record d'identité de l'invité (`Runtime::resolve` → `dht_key(code)`), pas une
clé publique déjà connue. La résolution est solide dès qu'un nœud JOIGNABLE
partagé existe (le lookup exclut déjà les nœuds injoignables, `find_node` →
`closest_k` sans les `Failed` ; prouvé par `nat_code_resolution_e2e` et les
sondes 60-fillers / cross-relais). Elle échoue quand l'inviteur et l'invité
n'ont **aucun rendez-vous joignable commun** — cas typique de deux amis qui ne
s'amorcent QUE l'un sur l'autre, tous deux derrière un NAT symétrique : aucun
n'est joignable, l'inviteur ne peut atteindre aucun nœud détenant le record
(reproduit : `dht_nodes = 0`). Ouvrir un port fait de ce pair le rendez-vous.

**Correctif.** Nœuds d'amorçage/relais **par défaut**
(`NodeConfig::default_bootstrap`, peuplés via la variable `ACCORD_BOOTSTRAP` —
exécution ou build), fusionnés avec les pairs de l'utilisateur pour le seeding,
la reconnexion et le repli de résolution (`Runtime::all_bootstrap_peers`). Deux
amis rejoignent alors automatiquement un réseau commun — comme les bootstrap
nodes d'IPFS/BitTorrent, sans serveur central (ces nœuds ne voient que du
chiffré et ne font que router/relayer). Test : `nat_default_bootstrap_e2e`
(deux NAT symétriques, AUCUN amorçage manuel entre amis, R en défaut →
résolution du code + demande + acceptation + DM).

**Déploiement.** Le build/release doit fournir au moins une adresse d'entrée
joignable via `ACCORD_BOOTSTRAP="ip:port,ip:port"`. Sans rendez-vous partagé,
deux pairs tous deux en NAT symétrique restent structurellement injoignables
(voir limite ci-dessous) : c'est une contrainte du serverless, pas un défaut
d'implémentation.

## 4. Limites connues (documentées, non masquées)

- **Premier contact hors ligne** : si B est éteint au moment de l'invitation,
  la livraison attend que A et B soient en ligne simultanément (l'outbox de A
  réessaie). La boîte aux lettres DHT ne couvre que les amis existants —
  extension possible (boîte « premier contact » avec preuve de travail dédiée)
  hors périmètre ici.
- L'éligibilité relais par « port observé = port local » accepte les NAT
  préservant le port comme relais : joignables tant que leurs mappings vivent
  (sessions entretenues), mais moins stables qu'un nœud public. Borné par
  l'essai ordonné des candidats et le repli.
- Un réseau v0 sans **aucun** nœud éligible relais (tous NATés symétriques,
  zéro UPnP) n'a pas de chemin de premier contact — limite structurelle : il
  faut au moins un pair joignable quelque part, qui n'a rien de central ni de
  dédié (n'importe quelle box avec UPnP ou n'importe quel nœud public suffit).
