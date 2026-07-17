# Nœuds de rendez-vous (bootstrap) — opération

Le premier contact « zéro configuration » entre deux utilisateurs tous deux
derrière un NAT symétrique exige un rendez-vous JOIGNABLE COMMUN
(`docs/NAT-FIRST-CONTACT.md`, §3ter) : sans lui, la résolution du code ami
échoue (« Code ami introuvable »). Le binaire `accord-bootstrap` fournit ce
rendez-vous : routage DHT + service de relais (SPEC §10-§11.3), **sans compte
utilisateur** — il ne voit que du trafic chiffré et des enregistrements DHT.

## Combien d'hôtes ?

**Deux hôtes distincts minimum, sur deux IP publiques distinctes.** La
classification du NAT côté client (M1b) recoupe les adresses publiques
observées par PLUSIEURS pairs : avec un seul observateur, `nat_kind` reste
`Unknown` et la stratégie de premier contact se dégrade. Deux hôtes sont le
minimum de production ; trois donnent de la marge en cas de panne.

Chaque hôte doit être maillé avec les autres (variable `ACCORD_BOOTSTRAP`
pointant vers les AUTRES hôtes) pour que la DHT forme un seul réseau.

## Binaire

```sh
cargo build --release -p accord-node --bin accord-bootstrap
```

Aucune dépendance native (le sous-système voix est compilé en mode simulé).

### Variables d'environnement

| Variable | Défaut | Rôle |
| --- | --- | --- |
| `ACCORD_BOOTSTRAP_STATE` | `./accord-bootstrap-state` | Répertoire d'état (identité de nœud + base + phrase de passe machine, 0600). |
| `ACCORD_BOOTSTRAP_P2P_ADDR` | `0.0.0.0:48016` | Écoute UDP (l'écouteur TCP de repli partage le même port). |
| `ACCORD_BOOTSTRAP` | vide | Autres rendez-vous à mailler, format `ip:port,ip:port`. |
| `ACCORD_BOOTSTRAP_PASSPHRASE` | générée | Phrase de passe explicite (sinon générée et persistée dans l'état). |
| `ACCORD_BOOTSTRAP_POW_BITS` | protocole | Difficulté PoW exigée des pairs. |
| `ACCORD_BOOTSTRAP_NAT` | `0` | `1` : mapping de port UPnP/NAT-PMP (hôte non public). |
| `ACCORD_BOOTSTRAP_MDNS` | `0` | `1` : annonce mDNS sur le LAN. |

L'identité de nœud est créée au premier lancement et réutilisée ensuite
(`node_id` stable). La perte de l'état est bénigne : les clients adressent un
rendez-vous par `ip:port`, pas par identité — repartir d'un état vierge suffit.

## Pare-feu

Ouvrir **UDP 48016** et **TCP 48016** (repli TCP, SPEC §11.3) en entrée.

## Déploiement Docker (par hôte)

```sh
cd deploy/bootstrap
ACCORD_BOOTSTRAP="IP_DE_L_AUTRE_HOTE:48016" docker compose up -d --build
```

`network_mode: host` : indispensable pour que l'adresse source UDP observée
soit celle de l'hôte (le NAT Docker fausserait les `ObservedAddr`).

## Déploiement systemd (par hôte)

```sh
cargo build --release -p accord-node --bin accord-bootstrap
install -m 755 target/release/accord-bootstrap /usr/local/bin/
install -m 644 deploy/bootstrap/accord-bootstrap.service /etc/systemd/system/
systemctl edit accord-bootstrap   # Environment=ACCORD_BOOTSTRAP=AUTRE_HOTE:48016
systemctl daemon-reload && systemctl enable --now accord-bootstrap
```

## Vérification

Au démarrage, le journal affiche :

```
nœud de rendez-vous démarré (DHT + relais, sans compte) p2p=0.0.0.0:48016 node_id=…
```

Depuis un poste client : `ACCORD_BOOTSTRAP="hote1:48016,hote2:48016"` dans
l'environnement de l'app, puis Paramètres → Réseau : les deux rendez-vous
apparaissent comme pairs, `dht_nodes ≥ 2`, et `nat_kind` se résout (≠ Unknown)
après quelques échanges.

## Câblage des releases

Le workflow `.github/workflows/release.yml` injecte le secret de dépôt
`ACCORD_BOOTSTRAP` (format `ip:port,ip:port` — les DEUX hôtes) dans l'env du
build Tauri : la constante de build (`option_env!("ACCORD_BOOTSTRAP")`) est
alors gravée dans les binaires distribués, et tout utilisateur dispose du
rendez-vous partagé sans configuration. Secret absent = comportement
d'aujourd'hui (aucun rendez-vous par défaut). La variable d'EXÉCUTION
`ACCORD_BOOTSTRAP` prime toujours sur la constante (ajustement sans
recompiler).
