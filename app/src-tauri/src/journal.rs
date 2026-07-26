//! Journal de diagnostic écrit sur disque (feuille de route §10.6).
//!
//! **Pourquoi ce module existe.** Une application graphique n'a pas de sortie
//! standard : lancée depuis le Finder ou le menu Démarrer, tout ce que
//! `tracing` produisait était perdu. Le diagnostic se faisait en demandant à
//! l'utilisateur de décrire ce qu'il avait vu.
//!
//! Trois décisions portent ce fichier.
//!
//! **1. Le tampon d'amorçage.** `tracing` doit être initialisé avant que Tauri
//! ne construise l'application, alors que le dossier de données n'est connu
//! qu'ensuite (`app.path().app_data_dir()`). Plutôt que de recopier la
//! convention de chemin de Tauri — qui changerait sans prévenir — les
//! premières lignes sont gardées en mémoire, puis versées dans le fichier dès
//! qu'il est ouvert. Sans cela, l'amorçage, c'est-à-dire précisément là où les
//! démarrages ratés se produisent, serait la seule partie non journalisée.
//!
//! **2. La rotation au démarrage.** Le journal précédent devient `.1` au lieu
//! d'être écrasé. L'ancienne version tronquait à l'ouverture : le redémarrage
//! qui suit un plantage effaçait la trace de ce plantage, au moment exact où
//! on venait la chercher.
//!
//! **3. Le plafond.** Un journal qui remplit le disque est un bug, pas un
//! outil. Au-delà de [`TAILLE_MAX`], le fichier tourne en cours de route ;
//! avec le `.1` conservé, l'empreinte totale est bornée à deux fois cette
//! valeur, et rien d'autre ne s'accumule.
//!
//! 🔒 **Ce qui ne doit jamais y entrer.** Un journal ne vaut que s'il peut
//! être envoyé à quelqu'un, et il ne peut l'être que s'il est sûr à partager :
//! jamais de contenu de message, de clé, de code ami ni d'adresse d'un ami.
//! C'est une contrainte sur les appels `tracing::` de tout le dépôt, pas sur
//! ce module — il écrit ce qu'on lui donne.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{reload, EnvFilter};

/// Taille au-delà de laquelle le journal courant tourne, en octets.
const TAILLE_MAX: u64 = 5 * 1024 * 1024;

/// Octets de démarrage gardés en mémoire avant l'ouverture du fichier. Large
/// pour un amorçage normal, borné pour qu'un démarrage qui n'atteint jamais
/// `setup` ne gonfle pas indéfiniment.
const TAMPON_MAX: usize = 256 * 1024;

/// Nom du journal courant dans le dossier de logs.
const NOM: &str = "accord.log";

/// Nom du journal précédent.
const NOM_PRECEDENT: &str = "accord.log.1";

/// État interne partagé entre le fil qui écrit et l'attachement du fichier.
struct Etat {
    fichier: Option<std::fs::File>,
    /// Lignes d'avant l'ouverture du fichier.
    tampon: Vec<u8>,
    /// Octets écrits dans le fichier courant, pour la rotation en cours de
    /// route.
    ecrits: u64,
    dossier: Option<PathBuf>,
}

/// Écrivain partagé donné à `tracing`. Cloné à chaque ligne, sérialisé par le
/// mutex.
#[derive(Clone)]
pub struct EcrivainJournal(Arc<Mutex<Etat>>);

impl EcrivainJournal {
    fn nouveau() -> Self {
        Self(Arc::new(Mutex::new(Etat {
            fichier: None,
            tampon: Vec::new(),
            ecrits: 0,
            dossier: None,
        })))
    }

    /// Ouvre le journal dans `dossier`, après rotation, et y verse le tampon
    /// d'amorçage.
    ///
    /// Best-effort : si le dossier n'est pas créable ou le fichier pas
    /// ouvrable, on continue sans journal. Empêcher l'application de démarrer
    /// parce que son fichier de diagnostic est indisponible serait une
    /// inversion des priorités.
    fn attacher(&self, dossier: &Path) {
        if std::fs::create_dir_all(dossier).is_err() {
            return;
        }
        rotation(dossier);
        let Ok(mut fichier) = std::fs::File::create(dossier.join(NOM)) else {
            return;
        };
        let mut etat = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = fichier.write_all(&etat.tampon);
        etat.ecrits = etat.tampon.len() as u64;
        etat.tampon = Vec::new();
        etat.tampon.shrink_to_fit();
        etat.fichier = Some(fichier);
        etat.dossier = Some(dossier.to_path_buf());
    }

    /// Dossier du journal, une fois attaché.
    fn dossier(&self) -> Option<PathBuf> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .dossier
            .clone()
    }
}

impl Write for EcrivainJournal {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut etat = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some(fichier) = etat.fichier.as_mut() else {
            // Avant l'ouverture : on garde, jusqu'au plafond. Au-delà, on
            // laisse tomber les lignes SUIVANTES plutôt que les premières —
            // un démarrage qui échoue le fait au début.
            if etat.tampon.len() + buf.len() <= TAMPON_MAX {
                etat.tampon.extend_from_slice(buf);
            }
            return Ok(buf.len());
        };
        let n = fichier.write(buf)?;
        let _ = fichier.flush();
        etat.ecrits += n as u64;
        if etat.ecrits >= TAILLE_MAX {
            if let Some(dossier) = etat.dossier.clone() {
                rotation(&dossier);
                if let Ok(neuf) = std::fs::File::create(dossier.join(NOM)) {
                    etat.fichier = Some(neuf);
                    etat.ecrits = 0;
                }
            }
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut etat = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match etat.fichier.as_mut() {
            Some(f) => f.flush(),
            None => Ok(()),
        }
    }
}

/// Décale le journal courant en `.1`, en remplaçant le précédent.
///
/// Un seul historique conservé, à dessein : ce qu'on cherche après un
/// incident, c'est l'exécution qui vient de finir, pas celle d'avant-hier.
fn rotation(dossier: &Path) {
    let courant = dossier.join(NOM);
    if courant.exists() {
        let _ = std::fs::rename(&courant, dossier.join(NOM_PRECEDENT));
    }
}

/// Poignée de réglage du niveau, sans redémarrage (§10.6, point 4).
type PoigneeNiveau = reload::Handle<EnvFilter, tracing_subscriber::Registry>;

/// Ce que l'hôte garde du journal après l'initialisation.
#[derive(Clone)]
pub struct Journal {
    ecrivain: EcrivainJournal,
    niveau: Arc<PoigneeNiveau>,
}

impl Journal {
    /// Ouvre le journal dans `<app_data>/logs`, une fois ce dossier connu.
    pub fn attacher_sous(&self, app_data: &Path) {
        self.ecrivain.attacher(&app_data.join("logs"));
    }

    /// Dossier du journal, ou `None` s'il n'a pas pu être ouvert.
    pub fn dossier(&self) -> Option<PathBuf> {
        self.ecrivain.dossier()
    }

    /// Change le niveau de journalisation à chaud.
    ///
    /// Rend `false` sur un niveau inconnu — la valeur vient de l'interface,
    /// donc d'ailleurs, et un niveau incompris ne doit pas passer en silence
    /// pour « inchangé » alors que l'utilisateur croit avoir activé le mode
    /// détaillé.
    pub fn regler_niveau(&self, niveau: &str) -> bool {
        let filtre = match niveau {
            "trace" => LevelFilter::TRACE,
            "debug" => LevelFilter::DEBUG,
            "info" => LevelFilter::INFO,
            "warn" => LevelFilter::WARN,
            "error" => LevelFilter::ERROR,
            _ => return false,
        };
        self.niveau
            .reload(EnvFilter::default().add_directive(filtre.into()))
            .is_ok()
    }
}

/// Installe le sous-système de journalisation.
///
/// Le niveau initial suit `RUST_LOG` s'il est posé (un développeur garde la
/// main), sinon `info`.
pub fn init() -> Journal {
    let ecrivain = EcrivainJournal::nouveau();
    let depart = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let (filtre, niveau) = reload::Layer::new(depart);

    let vers_fichier = {
        let e = ecrivain.clone();
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(move || e.clone())
    };
    // Les deux sorties cohabitent : le fichier pour l'utilisateur, `stdout`
    // pour qui lance le binaire depuis un terminal. Sans la seconde, déboguer
    // en développement passerait par un `tail -f`.
    tracing_subscriber::registry()
        .with(filtre)
        .with(vers_fichier)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .init();

    Journal {
        ecrivain,
        niveau: Arc::new(niveau),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_tampon_damorcage_est_verse_a_l_ouverture() {
        // 🔒 Le démarrage est ce qu'on veut lire quand l'application ne
        // démarre pas. Il est écrit avant que le dossier soit connu ; sans le
        // tampon, ce serait la seule partie manquante du journal.
        let dir = tempfile::tempdir().unwrap();
        let mut e = EcrivainJournal::nouveau();
        e.write_all(b"ligne d'amorcage\n").unwrap();

        e.attacher(dir.path());

        let contenu = std::fs::read_to_string(dir.path().join(NOM)).unwrap();
        assert!(contenu.contains("ligne d'amorcage"));
    }

    #[test]
    fn un_redemarrage_ne_detruit_pas_le_journal_precedent() {
        // 🔒 Le défaut que ce module corrige. L'ancienne version tronquait à
        // l'ouverture : le redémarrage qui suit un plantage effaçait la trace
        // de ce plantage.
        let dir = tempfile::tempdir().unwrap();

        let mut premier = EcrivainJournal::nouveau();
        premier.attacher(dir.path());
        premier.write_all(b"execution qui plante\n").unwrap();
        premier.flush().unwrap();
        drop(premier);

        let mut second = EcrivainJournal::nouveau();
        second.attacher(dir.path());
        second.write_all(b"execution suivante\n").unwrap();
        second.flush().unwrap();

        let precedent = std::fs::read_to_string(dir.path().join(NOM_PRECEDENT)).unwrap();
        let courant = std::fs::read_to_string(dir.path().join(NOM)).unwrap();
        assert!(
            precedent.contains("execution qui plante"),
            "la trace du plantage a été écrasée par le redémarrage"
        );
        assert!(courant.contains("execution suivante"));
    }

    #[test]
    fn le_journal_tourne_au_dela_du_plafond() {
        let dir = tempfile::tempdir().unwrap();
        let mut e = EcrivainJournal::nouveau();
        e.attacher(dir.path());

        let bloc = vec![b'x'; 64 * 1024];
        let mut ecrits = 0u64;
        while ecrits < TAILLE_MAX + 128 * 1024 {
            e.write_all(&bloc).unwrap();
            ecrits += bloc.len() as u64;
        }
        e.flush().unwrap();

        // Deux fichiers au plus, chacun sous le plafond : l'empreinte est
        // bornée, ce qui est le seul point de la rotation en cours de route.
        let courant = std::fs::metadata(dir.path().join(NOM)).unwrap().len();
        assert!(
            courant < TAILLE_MAX,
            "le journal courant dépasse le plafond"
        );
        assert!(dir.path().join(NOM_PRECEDENT).exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn le_tampon_damorcage_est_borne() {
        // Un démarrage qui n'atteint jamais `setup` ne doit pas gonfler sans
        // fin en mémoire.
        let mut e = EcrivainJournal::nouveau();
        let bloc = vec![b'y'; 32 * 1024];
        for _ in 0..20 {
            e.write_all(&bloc).unwrap();
        }
        let taille = e.0.lock().unwrap().tampon.len();
        assert!(taille <= TAMPON_MAX, "tampon non borné : {taille}");
    }
}
