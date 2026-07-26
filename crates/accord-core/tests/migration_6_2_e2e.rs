//! Migration d'un profil **réellement écrit par la 6.2.0** jusqu'au schéma
//! courant.
//!
//! 🔒 Pourquoi une fixture binaire plutôt qu'un schéma reconstruit à la main.
//! Les tests de migration de `db/mod.rs` sont excellents sur la mécanique —
//! numérotation, ordre, transaction, sauvegarde, refus d'une base plus récente
//! — mais ils partent tous d'une base fabriquée par le code *courant*, ou d'un
//! schéma minimal écrit dans le test. Aucun ne part de ce qu'un utilisateur a
//! réellement sur son disque. Or c'est précisément là que les migrations
//! échouent : sur une colonne oubliée, un index absent, un `PRAGMA` qui n'était
//! pas le même. Un schéma reconstruit de mémoire teste la mémoire, pas la
//! migration.
//!
//! Le fichier `fixtures/profil-6.2.0.db` a donc été produit par le code de la
//! 6.2.0 lui-même (`git worktree` sur le tag, `Db::open` puis écritures via
//! l'API publique de l'époque), et committé tel quel.
//!
//! ⚠️ C'est le chemin qu'a pris **chaque utilisateur existant** en passant à la
//! 7.0 : la 6.2.0 livrait le schéma 12, et rien entre les deux n'a été publié
//! qui aurait migré à sa place — la 6.3.0 et la 6.4.0 sont sorties le même
//! jour. Ce test est la seule chose qui prouve que ce saut se passe bien.

use accord_core::db::{ContactState, Db};

/// Clé de la base de la fixture. Constante et sans secret : elle ne protège
/// qu'un jeu de données de test.
const FIXTURE_KEY: [u8; 32] = [0x42u8; 32];

/// Schéma livré par la 6.2.0.
const SCHEMA_6_2_0: i64 = 12;

/// Copie la fixture dans un répertoire temporaire.
///
/// La migration **écrit** dans la base et dépose une sauvegarde à côté : la
/// faire sur le fichier committé le modifierait, et le test ne serait vrai
/// qu'une fois.
fn fixture_temporaire() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/profil-6.2.0.db");
    let cible = dir.path().join("core.db");
    std::fs::copy(&source, &cible).expect("copie de la fixture");
    (dir, cible)
}

#[test]
fn un_profil_6_2_migre_jusquau_schema_courant_sans_rien_perdre() {
    let (_dir, path) = fixture_temporaire();

    // La fixture est bien à la version de la 6.2.0 avant qu'on y touche : si
    // elle avait été régénérée par une version plus récente, ce test ne
    // prouverait plus rien et passerait quand même.
    {
        let brute = rusqlite::Connection::open(&path).expect("ouverture brute");
        brute
            .pragma_update(None, "key", format!("x'{}'", hex(&FIXTURE_KEY)))
            .expect("clé SQLCipher");
        let version: i64 = brute
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("version du schéma");
        assert_eq!(
            version, SCHEMA_6_2_0,
            "la fixture doit être une base 6.2.0, pas une base régénérée"
        );
    }

    // L'ouverture applique la chaîne complète.
    let db = Db::open(&path, &FIXTURE_KEY).expect("migration du profil 6.2");

    // 1. Le contact a survécu, avec ses colonnes d'époque.
    let contact = db
        .contact(&[1u8; 32])
        .expect("lecture du contact")
        .expect("le contact de la 6.2 doit survivre");
    assert_eq!(contact.display_name, "Alice d'avant");
    assert_eq!(contact.state, ContactState::Friend);
    assert_eq!(
        contact.verified_at,
        Some(1_700_000_050_000),
        "la vérification manuelle ne doit pas être perdue en route"
    );

    // 2. Les messages aussi, tous, et avec leur contenu.
    let historique = db
        .dm_history(&[1u8; 32], u64::MAX, 100)
        .expect("historique lisible après migration");
    assert_eq!(historique.len(), 3, "les trois messages doivent survivre");
    for (n, m) in historique.iter().rev().enumerate() {
        assert_eq!(
            m.body,
            format!("message {n} d'avant la migration").into_bytes(),
            "le corps du message {n} a changé pendant la migration"
        );
    }

    // 3. La file d'attente hors ligne — c'est du contenu non encore livré,
    // donc la perdre perdrait des messages que l'utilisateur croit envoyés.
    let en_file = db.outbox_for(&[9u8; 32]).expect("file lisible");
    assert_eq!(en_file.len(), 1, "la file hors-ligne doit survivre");
    assert_eq!(en_file[0].payload, b"charge en file");
}

#[test]
fn la_migration_installe_ce_dont_le_multi_appareil_a_besoin() {
    // 🔒 L'autre moitié : migrer sans perdre ne suffit pas, il faut aussi que
    // le profil migré soit utilisable par la version courante. Un profil 6.2
    // n'a ni identité d'appareil, ni cache de listes, ni marqueur de messages
    // rattrapés — tout ce sur quoi le jalon 1 repose.
    let (_dir, path) = fixture_temporaire();
    let db = Db::open(&path, &FIXTURE_KEY).expect("migration du profil 6.2");

    // L'appareil local n'existe pas encore : c'est le démarrage du nœud qui le
    // crée (`device::ensure_local_device`). Ce qui doit exister, c'est la
    // place pour l'accueillir.
    assert!(
        db.local_device()
            .expect("table local_device interrogeable")
            .is_none(),
        "un profil 6.2 n'a pas d'appareil, et la migration n'en invente pas"
    );

    // Le cache de listes d'appareils répond, vide.
    assert!(db
        .device_list(&[1u8; 32])
        .expect("table device_lists interrogeable")
        .is_none());

    // Et le marqueur des messages arrivés par rattrapage (migration 14) :
    // interrogeable pour ce pair, et vide, puisque rien n'a été rattrapé.
    assert!(db
        .dm_synced_set(&[1u8; 32])
        .expect("table dm_synced interrogeable")
        .is_empty());
}

/// Encodage hexadécimal minimal, pour le `PRAGMA key`.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
