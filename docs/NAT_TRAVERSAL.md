# NAT traversal — conception (SPEC §11)

Objectif : deux amis derrière des box/NAT se connectent **sans aucune
configuration de routeur**, à la manière de Tox/qTox, et sans serveur central.

## Vue d'ensemble des chemins, du meilleur au pire

1. **LAN direct** — découverte mDNS (`discovery.rs`), candidats locaux.
2. **Public direct** — port ouvert par UPnP-IGD ou NAT-PMP/PCP (`node/nat.rs`),
   ou redirection manuelle : l'adresse externe est publiée telle quelle.
3. **Poinçonnage UDP** — coordonné (voir ci-dessous) ou opportuniste (les deux
   côtés poinçonnent lors de la résolution de présence DHT).
4. **Poinçonnage TCP** — ouverture simultanée, quand l'UDP échoue (UDP filtré).
5. **Relais** — circuit chiffré via un nœud ami annonçant le drapeau relais
   (SPEC §10) ; le relais ne voit que des blobs. Dernier recours, toujours
   fonctionnel.

Le premier chemin qui aboutit gagne ; l'échec de chacun dégrade proprement vers
le suivant. Le comportement historique (LAN / redirection manuelle du port
48016) est inchangé et reste le repli de base.

## 1. Découverte de l'adresse externe (STUN sans serveur)

Pas de serveur STUN : les **pairs jouent ce rôle**. `ObserveAddrReq/Resp`
(canal CONTROL) demande à ≥ 3 pairs l'adresse source qu'ils observent ; le
runtime agrège (`ObservedAddrs`) et retient le consensus (≥ 2 concordants).
S'y ajoutent l'adresse mappée UPnP/NAT-PMP et les IP des interfaces locales.
L'ensemble, borné et dédupliqué, forme les **candidats** publiés dans le record
de présence DHT (`presence_addrs`).

## 2. Classification du NAT

`classify_nat` (`node/relay.rs`) recoupe les observations :
- consensus ⇒ **cone** (l'adresse publique est réutilisable, poinçonnage
  direct viable) ;
- divergence ⇒ **symétrique** (mapping par destination : le poinçonnage direct
  est improbable, on privilégie le relais) ;
- < 2 observations ⇒ inconnu.

Exposé à l'UI par `network.status.nat_kind`.

## 3. Poinçonnage UDP coordonné

### Rendez-vous sans serveur

Deux mécanismes complémentaires, tous deux décentralisés :

- **Baseline (existant)** : chaque côté résout la présence DHT de l'ami et
  poinçonne ses candidats à chaque passe de maintenance. Fonctionne sans
  coordination mais avec des candidats potentiellement vieux et des salves non
  synchronisées.
- **Coordonné (nouveau, SPEC §11.2)** : dès qu'un lien EXISTE avec l'ami —
  typiquement la session bout-en-bout **tunnelée par un relais**, établie en
  quelques secondes — il sert de canal de signalisation chiffré :

  ```
  A ──PunchRequest{token, candidats_A}──▶ B   (via relais ou session existante)
  A ◀─PunchResponse{token, candidats_B}── B   puis B poinçonne candidats_A
  A poinçonne candidats_B                     les salves HELLO se croisent
  ```

  Les candidats échangés sont **frais** (à la seconde), les salves quasi
  simultanées (décalage ≈ RTT/2 pour des salves de ~1 s : recouvrement
  garanti). Les HELLO simultanés sont résolus en une session unique par
  l'arbitrage déterministe de `Endpoint::on_hello`, avec liaison d'identité
  anti-MITM (D-037) des deux côtés.

Le déclencheur vit dans la maintenance : après le repli relais
(`PUNCH_FALLBACK_MS`), une demande d'**upgrade relais → direct** est émise
(cadencée). Correctif associé : `Endpoint::punch` ne s'arrête plus sur une
session *relayée* (`has_direct_session_with`), sinon l'upgrade était
impossible.

### Garde-fous (`node/holepunch.rs`)

Un pair de session, même ami authentifié, est traité comme hostile :

| Menace | Parade |
|---|---|
| Faire arroser des tiers (scan/flood induit) | candidats ≤ 8 (borne au décodage), filtrés (`sanitize_candidates` : non-spécifiée, multicast, broadcast, loopback, port 0 exclus) ; une salve = 5 petits HELLO/candidat |
| Demandes en rafale | cadence par pair (10 s entrant, 30 s sortant), état global borné (256 pairs, purge par ancienneté) |
| `PunchResponse` forgée/rejouée | ignorée sauf jeton d'une demande sortante fraîche (TTL 30 s), consommé une seule fois |
| Demande d'un non-ami (nœud DHT quelconque) | ignorée (amitié requise avant toute émission) |

Résidu assumé : un ami peut faire émettre quelques HELLO vers des adresses
LAN privées (même exposition que la baseline par présence DHT, bornée).

## 4. Repli TCP (`transport/tcp.rs`)

Quand l'UDP ne passe pas (réseaux qui filtrent l'UDP), le **même protocole
paquet** transite sur TCP :

- **Trames** `[len u16 BE][paquet]`, `1..=2048` octets — au-delà, lien fermé,
  aucune allocation pilotée par l'attaquant.
- **`MuxSocket`** : un `DatagramSocket` qui multiplexe le socket UDP réel et
  un registre de liens TCP. L'endpoint ne change pas : handshake, PoW,
  chiffrement, keep-alive et anti-DoS s'appliquent à l'identique.
- **Écouteur TCP** sur le même port que l'UDP (`SO_REUSEADDR`/`SO_REUSEPORT`),
  partagé avec les `connect()` de poinçonnage.
- **Ouverture simultanée** : après l'échec de la salve UDP, chaque côté (la
  coordination §11.2 les synchronise) tente `connect()` vers tous les candidats
  en parallèle depuis son port local, en 3 rondes. SYN croisés ⇒ connexion ;
  sinon l'écouteur capte le SYN entrant. Première connexion adoptée, handshake
  de session rejoué à travers elle.
- **Bornes** : 64 liens au total, 4 par IP distante, files d'écriture/lecture
  bornées (dépassement = trame perdue, sémantique datagramme), inactivité
  120 s ⇒ fermeture. TCP prouve l'adresse source : pas d'amplification
  possible vers une victime usurpée.

**Limites documentées, non masquées :**

- Sans observation du port TCP public du pair (pas de STUN-TCP), chaque côté
  vise le **port annoncé** : la traversée TCP réussit surtout avec des NAT
  préservant le port, une redirection existante, ou quand seul UDP est filtré.
  C'est le « best effort » de la SPEC §11.3.
- **Symétrique ↔ symétrique** : ni l'UDP ni le TCP ne passent en général — le
  **relais reste requis** (équivalent TURN décentralisé). C'est structurel,
  pas un défaut d'implémentation.
- Si un lien TCP meurt, les envois retombent sur UDP vers la même adresse
  (souvent injoignable) jusqu'à l'expiration keep-alive de la session, puis le
  cycle normal reprend (re-poinçonnage / relais).
- Un botnet distribué peut saturer le registre de liens TCP (64) : seul le
  REPLI TCP est alors dégradé, les chemins UDP et relais sont intacts.
- Le mapping UPnP ne couvre que l'UDP ; un mapping TCP dédié serait un gain
  marginal (si UPnP marche, l'UDP direct marche déjà) — non retenu.

## 5. Premier contact sans port ouvert (`NODE_ANNOUNCE`, éligibilité relais)

Voir [`NAT-FIRST-CONTACT.md`](NAT-FIRST-CONTACT.md) pour le diagnostic complet
(le rendez-vous relais domicile était conçu mais mort en production). En bref :

- **`NODE_ANNOUNCE` (CONTROL 0x08)** : auto-annonce `{pow_nonce, flags}` à
  l'établissement de chaque session DIRECTE (initiateur, puis réponse unique du
  répondeur). Le récepteur reconstruit un `NodeInfo` — identité authentifiée
  par la session, adresse OBSERVÉE, PoW re-vérifiée — et peuple sa table de
  routage : c'est l'apprentissage organique qui rend `home_relays_of` et
  `select_relay_for` non vides en conditions réelles. Jamais sur un tunnel
  (l'adresse du relais empoisonnerait la table).
- **Éligibilité relais** (`relay_eligible`) : RELAY annoncé seulement si
  mapping de port actif ou consensus d'adresse observée au port local ;
  ré-annonce aux sessions directes au changement. Un nœud NATé symétrique ne
  s'annonce jamais relais (les relais domicile élus doivent être joignables).
- **Repli relais inconditionnel** : déclenché pour chaque cible de résolution
  même sans record de présence (le rendez-vous domicile ne dépend que de la
  clé publique) ; `ensure_relay_to` et `home_relay_tick` font un `lookup_node`
  réseau avant sélection (convergence des deux côtés sur la vue globale).
- **Livraison sur lien établi** (`send_via_best_link`) : l'outbox, le vidage à
  la connexion, le profil et l'anti-entropie n'émettent que sur une session
  directe établie ou un circuit relais — jamais via le handshake spéculatif,
  dont le `Ok` faisait disparaître des messages en file vers des adresses
  injoignables.

Test reproductible : `crates/accord-node/tests/nat_first_contact_e2e.rs`
(A et B derrière deux NAT symétriques simulés — mapping par destination,
aucun entrant non sollicité —, R seul nœud public : demande d'ami, amitié et
DM aller-retour sans aucune ouverture de port).

## Compatibilité filaire

`ControlMsg` 0x06/0x07/0x08 sont additifs : un pair ancien qui les reçoit les
rejette au décodage (`MALFORMED`, silencieux par SPEC §12) et le comportement
retombe sur la baseline. Aucun champ existant n'est modifié.

## Fichiers touchés

| Zone | Fichiers | Contenu |
|---|---|---|
| Protocole | `accord-proto` (`plaintext.rs`, `limits.rs`) | `PunchRequest`/`PunchResponse`, borne candidats |
| Transport | `accord-transport` (`tcp.rs` nouveau, `endpoint.rs`, `lib.rs`) | repli TCP, événements punch, `has_direct_session_with` |
| Nœud | `accord-node` (`node/holepunch.rs` nouveau, `runtime.rs`, `maintenance.rs`, `lib.rs`) | politique, coordination, écouteur TCP, déclencheur d'upgrade |

Tests : bornes protocolaires (`roundtrip.rs`), unités TCP + admission
(`tcp.rs`), session complète sur TCP + octets forgés (`tcp_link_e2e.rs`),
politique de coordination (`holepunch.rs`).
