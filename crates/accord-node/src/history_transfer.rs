//! Transfert d'historique à l'appairage (feuille de route §17.4).
//!
//! Un appareil fraîchement appairé demande à un autre appareil du compte de lui
//! renvoyer **tout** l'historique, conversation par conversation, en descendant
//! du message le plus récent qu'il détient vers les plus anciens.
//!
//! 🔴 **À ne pas confondre avec le rattrapage** (`node/dm_sync.rs`), qui ne sert
//! jamais que ses 64 messages les plus récents. Les deux se ressemblent sur le
//! fil — la réponse est faite des mêmes `SelfSyncItem` — et n'ont ni le même
//! but ni les mêmes limites. Voir `CoreMsg::SelfHistoryPull` pour pourquoi le
//! rattrapage ne pouvait pas simplement être « piloté en boucle ».
//!
//! Extrait de `runtime.rs`, qui est un fichier de la dette : il ne doit pas
//! grossir.

use std::time::Duration;

use accord_proto::core_msg::MAX_SELF_SYNC_ITEMS;
use accord_proto::plaintext::ChannelMsg;

use crate::runtime::Runtime;

impl Runtime {
    /// Tire **tout** l'historique depuis un autre appareil du compte
    /// (feuille de route §17.4).
    ///
    /// ## Comment la boucle sait qu'elle a fini
    ///
    /// Chaque passe demande, pour une conversation, la page immédiatement plus
    /// ancienne que ce qu'on détient déjà — la borne se lit dans la base, pas
    /// dans un curseur tenu à part (voir `Db::dm_lowest_lamport`). Après
    /// l'envoi, on laisse au frère le temps de répondre, puis on regarde si
    /// cette borne a **baissé**.
    ///
    /// 🔒 **Deux passes sans nouveauté valent fin**, et non un marqueur de fin
    /// envoyé par le frère. Un marqueur serait un unique datagramme dont la
    /// perte laisserait l'écran d'attente tourner indéfiniment — et ces
    /// messages ne sont PAS mis en file hors ligne
    /// (`maintenance::is_queueable_offline`), donc sa perte est un cas courant,
    /// pas un cas rare. Deux passes, parce qu'une seule confondrait « il n'y a
    /// plus rien » avec « le datagramme s'est perdu » : la seconde chance coûte
    /// une attente et supprime tout un mode de panne.
    ///
    /// ⚠️ Ce que cette fonction ne fait PAS : distinguer « le frère ne connaît
    /// pas cet opcode » (version plus ancienne) de « le frère n'a rien de plus
    /// ancien ». Les deux se présentent comme une passe sans nouveauté. C'est
    /// à l'appelant de le dire à l'utilisateur — un transfert qui rend zéro
    /// message alors que le carnet n'est pas vide est le signal.
    pub(crate) async fn transfer_history_from(&self, device: &[u8; 32]) -> (usize, usize) {
        /// Pas d'interrogation de la base pendant l'attente d'une page.
        const PAS: Duration = Duration::from_millis(100);
        /// Attente maximale d'une page avant de la compter perdue.
        const ATTENTE: Duration = Duration::from_secs(8);
        /// Pages perdues d'affilée avant d'abandonner une conversation.
        const PERTES: u8 = 2;

        let convs = match self.node.history_conversations() {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(erreur = %e, "transfert d'historique : carnet illisible");
                return (0, 0);
            }
        };
        let total = convs.len();
        let mut pages = 0usize;
        for (index, conv) in convs.iter().enumerate() {
            let mut perdues = 0u8;
            while perdues < PERTES {
                let demande = match self.node.next_history_pull(conv) {
                    Ok(Some(m)) => m,
                    // Plancher atteint : il n'existe rien de plus ancien.
                    Ok(None) => break,
                    Err(e) => {
                        tracing::debug!(erreur = %e, "transfert : demande illisible");
                        break;
                    }
                };
                let avant = self.node.history_count(conv);
                if !self
                    .send_via_best_link(device, &ChannelMsg::Core(demande))
                    .await
                {
                    // Lien mort. On attend avant de recompter : sans cette
                    // attente, les deux tentatives se consommaient en quelques
                    // microsecondes et le transfert se déclarait fini avant que
                    // la session ait eu le temps de s'établir — c'est-à-dire
                    // systématiquement quand l'utilisateur clique juste après
                    // le démarrage.
                    tokio::time::sleep(ATTENTE).await;
                    perdues += 1;
                    continue;
                }
                // Interrogation courte plutôt qu'une attente fixe : on repart
                // dès que la page est là, et on ne la déclare perdue qu'après
                // la borne entière.
                let mut recu = 0usize;
                let mut reste = ATTENTE;
                while reste > Duration::ZERO {
                    tokio::time::sleep(PAS).await;
                    reste = reste.saturating_sub(PAS);
                    recu = self.node.history_count(conv).saturating_sub(avant);
                    if recu > 0 {
                        break;
                    }
                }
                if recu == 0 {
                    perdues += 1;
                    continue;
                }
                pages += 1;
                perdues = 0;
                self.node
                    .emit_history_progress(index + 1, total, pages, false);
                // 🔒 **C'est ici que la boucle sait qu'elle a fini**, et c'est
                // une information, pas un minuteur. Une page servie INCOMPLÈTE
                // ne peut signifier qu'une chose : la source n'a plus rien de
                // plus ancien à donner — elle sert toujours autant qu'elle
                // peut, jusqu'à la borne demandée.
                //
                // La version d'avant s'arrêtait au bout de deux attentes sans
                // nouveauté. Sous charge, ces attentes expiraient alors que la
                // page arrivait encore : le transfert annonçait « terminé »
                // avec 64 messages sur 90. Un transfert partiel présenté comme
                // complet est exactement le défaut qu'on reprochait au plan
                // d'origine ; le remplacer par un délai plus long n'aurait fait
                // que déplacer le seuil.
                if recu < usize::from(MAX_SELF_SYNC_ITEMS) {
                    break;
                }
            }
            self.node
                .emit_history_progress(index + 1, total, pages, false);
        }
        self.node.emit_history_progress(total, total, pages, true);
        tracing::info!(
            conversations = total,
            pages,
            "transfert d'historique terminé"
        );
        (total, pages)
    }
}
