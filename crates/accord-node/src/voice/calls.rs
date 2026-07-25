//! Machine d'états des appels vocaux 1-à-1 (sonnerie, acceptation, refus,
//! occupé, timeout). Pure : aucune E/S ni horloge propre — le moteur voix
//! ([`super::engine`]) fournit `now_ms` et exécute les [`CallAction`]
//! décidées ici, ce qui rend chaque transition testable au tick près.
//!
//! Sécurité (P2P public) :
//! - une offre entrante n'est honorée que d'un AMI confirmé (vérifié par le
//!   moteur AVANT d'appeler [`CallMachine::on_offer`]) ;
//! - cadence par pair : au plus une NOUVELLE sonnerie par
//!   [`NEW_RING_MIN_INTERVAL_MS`] et une réponse « occupé » par
//!   [`BUSY_REPLY_MIN_INTERVAL_MS`] (zéro amplification : un non-ami ne
//!   déclenche jamais aucune réponse, un ami au plus un petit message par
//!   fenêtre) ;
//! - anti-rejeu : `CallAnswer`/`CallDecline`/`CallHangup` ne sont honorés que
//!   s'ils corrèlent exactement l'appel courant (pair émetteur + `call_id`) ;
//!   tout le reste est ignoré en silence ;
//! - suivi par pair borné ([`PEER_TRACKING_MAX`]) : pas de croissance mémoire
//!   sous pseudo-identités.

use std::collections::HashMap;

use accord_proto::core_msg::CALL_DECLINE_BUSY;

/// Durée de sonnerie avant abandon, des deux côtés (ms).
pub(crate) const RING_TIMEOUT_MS: u64 = 45_000;

/// Période de réémission de l'offre pendant la sonnerie sortante (ms) : les
/// offres voyagent en datagrammes avec pertes, l'appelé déduplique par
/// `call_id`.
pub(crate) const OFFER_RESEND_MS: u64 = 2_000;

/// Intervalle minimal entre deux NOUVELLES sonneries entrantes d'un même pair
/// (ms) — anti sonnerie-spam ; les réémissions d'une même offre (`call_id`
/// identique) ne comptent pas.
pub(crate) const NEW_RING_MIN_INTERVAL_MS: u64 = 3_000;

/// Intervalle minimal entre deux réponses « occupé » à un même pair (ms) :
/// borne l'amplification à moins d'un petit message par offre reçue.
pub(crate) const BUSY_REPLY_MIN_INTERVAL_MS: u64 = 2_000;

/// Silence toléré sur une sonnerie entrante avant de conclure qu'elle n'a plus
/// d'objet (ms).
///
/// 🔒 C'est le filet du multi-appareil, et il ne dépend d'**aucun** message
/// reçu. L'appelant réémet son offre toutes les [`OFFER_RESEND_MS`] tant que ça
/// sonne, et cesse net dès qu'il décroche : un appareil qui sonne encore sans
/// plus rien recevoir en conclut de lui-même. Sans ce filet, la perte du
/// message « décroché ailleurs » ferait sonner les autres appareils pendant les
/// [`RING_TIMEOUT_MS`] complètes, puis afficher un **appel manqué** pour un
/// appel qui a été pris — ce qui est pire que de ne pas sonner du tout.
///
/// Quatre périodes : assez pour absorber une rafale de pertes, assez court pour
/// que la sonnerie résiduelle reste de l'ordre de la seconde.
pub(crate) const RING_STALE_MS: u64 = 4 * OFFER_RESEND_MS;

/// Nombre d'émissions de « décroché ailleurs » après une réponse honorée.
///
/// Le message voyage en UDP ; en répéter quelques-uns coûte trois petits
/// datagrammes et raccourcit la sonnerie résiduelle d'une poignée de secondes
/// dans le cas courant. Ce n'est qu'une optimisation de latence :
/// [`RING_STALE_MS`] garantit la correction même s'ils se perdent tous.
pub(crate) const TAKEN_RESENDS: u8 = 3;

/// Borne du suivi de cadence par pair (au-delà, table réinitialisée — même
/// motif que le suivi de débit du service de fichiers).
const PEER_TRACKING_MAX: usize = 256;

/// Phase d'un appel, exposée par `calls.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPhase {
    /// Aucun appel.
    Idle,
    /// Notre offre sonne chez le pair.
    OutgoingRinging,
    /// L'offre d'un pair sonne chez nous.
    IncomingRinging,
    /// Appel accepté, session audio en cours.
    Active,
}

impl CallPhase {
    /// Libellé stable du contrat API (`calls.status`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::OutgoingRinging => "outgoing_ringing",
            Self::IncomingRinging => "incoming_ringing",
            Self::Active => "active",
        }
    }
}

/// Photographie de l'appel courant (`calls.status`).
#[derive(Debug, Clone, Copy)]
pub struct CallSnapshot {
    /// Phase courante.
    pub phase: CallPhase,
    /// Pair de l'appel (absent au repos).
    pub peer: Option<[u8; 32]>,
    /// Identifiant de l'appel (absent au repos).
    pub call_id: Option<[u8; 16]>,
    /// Début de la phase courante (ms de l'horloge du moteur).
    pub since_ms: Option<u64>,
}

/// Action décidée par la machine, exécutée par le moteur voix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CallAction {
    /// Émettre `CallOffer` au pair.
    SendOffer {
        /// Destinataire.
        to: [u8; 32],
        /// Appel offert.
        call_id: [u8; 16],
    },
    /// Émettre `CallAnswer` au pair.
    SendAnswer {
        /// Destinataire.
        to: [u8; 32],
        /// Appel accepté.
        call_id: [u8; 16],
    },
    /// Émettre `CallDecline` au pair.
    SendDecline {
        /// Destinataire.
        to: [u8; 32],
        /// Appel refusé.
        call_id: [u8; 16],
        /// 0 = refusé, 1 = occupé.
        reason: u8,
    },
    /// Émettre `CallHangup` au pair.
    SendHangup {
        /// Destinataire.
        to: [u8; 32],
        /// Appel terminé.
        call_id: [u8; 16],
    },
    /// Émettre `CallTaken` au COMPTE du pair : ses autres appareils peuvent
    /// cesser de sonner. Le gagnant le reçoit aussi et l'ignore.
    SendTaken {
        /// Destinataire (compte).
        to: [u8; 32],
        /// Appel honoré.
        call_id: [u8; 16],
    },
    /// Démarrer la session audio de l'appel (salon `room == call_id`).
    JoinAudio {
        /// Pair de l'appel.
        peer: [u8; 32],
        /// Salon audio de l'appel.
        call_id: [u8; 16],
    },
    /// Quitter la session audio de l'appel.
    LeaveAudio,
    /// Émettre `event.call_incoming`.
    EventIncoming {
        /// Appelant.
        peer: [u8; 32],
        /// Appel offert.
        call_id: [u8; 16],
    },
    /// Émettre `event.call_outgoing`.
    EventOutgoing {
        /// Appelé.
        peer: [u8; 32],
        /// Appel offert.
        call_id: [u8; 16],
    },
    /// Émettre `event.call_accepted`.
    EventAccepted {
        /// Pair de l'appel.
        peer: [u8; 32],
        /// Appel accepté.
        call_id: [u8; 16],
    },
    /// Émettre `event.call_ended`.
    EventEnded {
        /// Pair de l'appel.
        peer: [u8; 32],
        /// Appel terminé.
        call_id: [u8; 16],
        /// Raison stable du contrat API.
        reason: &'static str,
    },
}

/// État interne.
#[derive(Debug)]
enum State {
    Idle,
    Outgoing {
        peer: [u8; 32],
        call_id: [u8; 16],
        started_ms: u64,
        last_offer_ms: u64,
    },
    Incoming {
        peer: [u8; 32],
        call_id: [u8; 16],
        received_ms: u64,
        /// Dernière offre reçue pour cette sonnerie — y compris les
        /// réémissions. Distinct de `received_ms`, qui fixe l'échéance et ne
        /// bouge jamais : rejouer une offre ne doit pas prolonger la sonnerie.
        last_offer_ms: u64,
    },
    Active {
        peer: [u8; 32],
        call_id: [u8; 16],
        connected_ms: u64,
        /// Émissions restantes de « décroché ailleurs » (voir
        /// [`TAKEN_RESENDS`]). Zéro quand c'est nous qui avons décroché : dans
        /// ce sens, c'est l'appelant qui prévient nos autres appareils.
        taken_left: u8,
        /// Dernière émission de « décroché ailleurs ».
        last_taken_ms: u64,
    },
}

/// Machine d'états d'appel (un seul appel à la fois).
pub(crate) struct CallMachine {
    /// Clé publique locale (résolution déterministe des appels croisés).
    me: [u8; 32],
    state: State,
    /// Dernière NOUVELLE sonnerie créée par pair (cadence, borné).
    last_ring_ms: HashMap<[u8; 32], u64>,
    /// Dernière réponse « occupé » émise par pair (cadence, borné).
    last_busy_ms: HashMap<[u8; 32], u64>,
}

impl CallMachine {
    /// Machine au repos.
    pub(crate) fn new(me: [u8; 32]) -> Self {
        Self {
            me,
            state: State::Idle,
            last_ring_ms: HashMap::new(),
            last_busy_ms: HashMap::new(),
        }
    }

    /// Photographie de l'appel courant.
    pub(crate) fn snapshot(&self) -> CallSnapshot {
        match &self.state {
            State::Idle => CallSnapshot {
                phase: CallPhase::Idle,
                peer: None,
                call_id: None,
                since_ms: None,
            },
            State::Outgoing {
                peer,
                call_id,
                started_ms,
                ..
            } => CallSnapshot {
                phase: CallPhase::OutgoingRinging,
                peer: Some(*peer),
                call_id: Some(*call_id),
                since_ms: Some(*started_ms),
            },
            State::Incoming {
                peer,
                call_id,
                received_ms,
                ..
            } => CallSnapshot {
                phase: CallPhase::IncomingRinging,
                peer: Some(*peer),
                call_id: Some(*call_id),
                since_ms: Some(*received_ms),
            },
            State::Active {
                peer,
                call_id,
                connected_ms,
                ..
            } => CallSnapshot {
                phase: CallPhase::Active,
                peer: Some(*peer),
                call_id: Some(*call_id),
                since_ms: Some(*connected_ms),
            },
        }
    }

    /// `calls.start` : lance un appel sortant (la vérification d'amitié est
    /// faite par le moteur AVANT cet appel). Erreur explicite si un appel est
    /// déjà en cours.
    pub(crate) fn start(
        &mut self,
        peer: [u8; 32],
        call_id: [u8; 16],
        now_ms: u64,
    ) -> Result<Vec<CallAction>, &'static str> {
        if !matches!(self.state, State::Idle) {
            return Err("appel déjà en cours");
        }
        self.state = State::Outgoing {
            peer,
            call_id,
            started_ms: now_ms,
            last_offer_ms: now_ms,
        };
        Ok(vec![
            CallAction::SendOffer { to: peer, call_id },
            CallAction::EventOutgoing { peer, call_id },
        ])
    }

    /// `calls.accept` : accepte la sonnerie entrante identifiée par `call_id`.
    pub(crate) fn accept(
        &mut self,
        call_id: [u8; 16],
        now_ms: u64,
    ) -> Result<Vec<CallAction>, &'static str> {
        let State::Incoming {
            peer,
            call_id: ringing,
            ..
        } = self.state
        else {
            return Err("aucun appel entrant à accepter");
        };
        if ringing != call_id {
            return Err("identifiant d'appel inconnu");
        }
        self.state = State::Active {
            peer,
            call_id,
            connected_ms: now_ms,
            // Zéro : c'est nous qui décrochons. Prévenir nos propres autres
            // appareils est le rôle de l'APPELANT, seul à savoir quelle
            // réponse il a honorée.
            taken_left: 0,
            last_taken_ms: now_ms,
        };
        Ok(vec![
            CallAction::SendAnswer { to: peer, call_id },
            CallAction::JoinAudio { peer, call_id },
            CallAction::EventAccepted { peer, call_id },
        ])
    }

    /// `calls.decline` : refuse la sonnerie entrante identifiée par `call_id`.
    pub(crate) fn decline(&mut self, call_id: [u8; 16]) -> Result<Vec<CallAction>, &'static str> {
        let State::Incoming {
            peer,
            call_id: ringing,
            ..
        } = self.state
        else {
            return Err("aucun appel entrant à refuser");
        };
        if ringing != call_id {
            return Err("identifiant d'appel inconnu");
        }
        self.state = State::Idle;
        Ok(vec![
            CallAction::SendDecline {
                to: peer,
                call_id,
                reason: accord_proto::core_msg::CALL_DECLINE_REJECTED,
            },
            CallAction::EventEnded {
                peer,
                call_id,
                reason: "declined",
            },
        ])
    }

    /// `calls.hangup` : termine l'appel courant quelle que soit sa phase
    /// (annulation d'une sonnerie sortante, refus d'une entrante, raccrochage
    /// d'un appel actif). Idempotent au repos.
    pub(crate) fn hangup(&mut self) -> Vec<CallAction> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Idle => vec![],
            State::Outgoing { peer, call_id, .. } => vec![
                CallAction::SendHangup { to: peer, call_id },
                CallAction::EventEnded {
                    peer,
                    call_id,
                    reason: "hangup",
                },
            ],
            State::Incoming { peer, call_id, .. } => vec![
                CallAction::SendDecline {
                    to: peer,
                    call_id,
                    reason: accord_proto::core_msg::CALL_DECLINE_REJECTED,
                },
                CallAction::EventEnded {
                    peer,
                    call_id,
                    reason: "declined",
                },
            ],
            State::Active { peer, call_id, .. } => vec![
                CallAction::SendHangup { to: peer, call_id },
                CallAction::LeaveAudio,
                CallAction::EventEnded {
                    peer,
                    call_id,
                    reason: "hangup",
                },
            ],
        }
    }

    /// Offre entrante d'un pair DÉJÀ vérifié ami par le moteur. Applique la
    /// cadence par pair, la déduplication par `call_id`, la réponse
    /// « occupé » bornée et la résolution déterministe des appels croisés.
    pub(crate) fn on_offer(
        &mut self,
        from: [u8; 32],
        call_id: [u8; 16],
        now_ms: u64,
    ) -> Vec<CallAction> {
        match &self.state {
            State::Idle => self.try_new_ring(from, call_id, now_ms),
            State::Incoming {
                peer,
                call_id: ringing,
                ..
            } => {
                if *peer == from && *ringing == call_id {
                    // Réémission de la même offre (pertes UDP) : dédupliquée,
                    // l'échéance d'origine est conservée (pas de sonnerie
                    // infinie en rejouant la même offre). Seule la date de
                    // DERNIÈRE offre avance — c'est elle qui alimente le filet
                    // de RING_STALE_MS, et elle seule.
                    if let State::Incoming { last_offer_ms, .. } = &mut self.state {
                        *last_offer_ms = now_ms;
                    }
                    return vec![];
                }
                if *peer == from {
                    // Nouvelle offre du même pair : l'ancienne sonnerie est
                    // périmée (annulation perdue en route). Remplacement,
                    // sous la même cadence qu'une sonnerie neuve.
                    let (old_peer, old_call) = (*peer, *ringing);
                    let mut actions = self.try_new_ring(from, call_id, now_ms);
                    if !actions.is_empty() {
                        actions.insert(
                            0,
                            CallAction::EventEnded {
                                peer: old_peer,
                                call_id: old_call,
                                reason: "canceled",
                            },
                        );
                    }
                    return actions;
                }
                // Un autre pair appelle pendant qu'on sonne déjà : occupé.
                self.busy_reply(from, call_id, now_ms)
            }
            State::Outgoing {
                peer,
                call_id: ours,
                ..
            } => {
                if *peer != from {
                    return self.busy_reply(from, call_id, now_ms);
                }
                // Appel croisé (chacun appelle l'autre) : les deux côtés
                // convergent déterministiquement vers l'appel de la plus
                // petite clé publique — l'un accepte l'offre de l'autre,
                // l'autre ignore l'offre reçue et verra arriver la réponse.
                if from < self.me {
                    let ours = *ours;
                    self.state = State::Active {
                        peer: from,
                        call_id,
                        connected_ms: now_ms,
                        taken_left: 0,
                        last_taken_ms: now_ms,
                    };
                    vec![
                        CallAction::EventEnded {
                            peer: from,
                            call_id: ours,
                            reason: "superseded",
                        },
                        CallAction::SendAnswer { to: from, call_id },
                        CallAction::JoinAudio {
                            peer: from,
                            call_id,
                        },
                        CallAction::EventAccepted {
                            peer: from,
                            call_id,
                        },
                    ]
                } else {
                    vec![] // Notre appel gagne : leur machine l'acceptera.
                }
            }
            State::Active {
                peer,
                call_id: current,
                ..
            } => {
                if *peer == from && *current == call_id {
                    // Notre réponse s'est perdue : le pair réémet son offre.
                    // On réémet la réponse (idempotent).
                    return vec![CallAction::SendAnswer { to: from, call_id }];
                }
                self.busy_reply(from, call_id, now_ms)
            }
        }
    }

    /// Réponse à notre offre : uniquement si elle corrèle l'appel sortant
    /// courant (pair + `call_id`), sinon ignorée (forgée, rejouée, périmée).
    pub(crate) fn on_answer(
        &mut self,
        from: [u8; 32],
        call_id: [u8; 16],
        now_ms: u64,
    ) -> Vec<CallAction> {
        let State::Outgoing {
            peer,
            call_id: ours,
            ..
        } = self.state
        else {
            return vec![];
        };
        if peer != from || ours != call_id {
            return vec![];
        }
        self.state = State::Active {
            peer,
            call_id,
            connected_ms: now_ms,
            // Une première annonce part tout de suite ; `tick` en réémettra
            // TAKEN_RESENDS - 1 autres, puisque le message peut se perdre.
            taken_left: TAKEN_RESENDS.saturating_sub(1),
            last_taken_ms: now_ms,
        };
        vec![
            CallAction::SendTaken { to: peer, call_id },
            CallAction::JoinAudio { peer, call_id },
            CallAction::EventAccepted { peer, call_id },
        ]
    }

    /// « Décroché ailleurs » : un autre appareil de ce compte a pris l'appel.
    ///
    /// N'agit **que** sur une sonnerie en cours qui corrèle exactement. Le
    /// gagnant reçoit le même message — il est adressé au compte, donc à tous
    /// les appareils — et l'ignore parce qu'il est déjà `Active`. Un message
    /// forgé ou rejoué ne peut donc qu'éteindre une sonnerie que son propre
    /// appelant a déjà quittée.
    pub(crate) fn on_taken(&mut self, from: [u8; 32], call_id: [u8; 16]) -> Vec<CallAction> {
        let State::Incoming {
            peer,
            call_id: ringing,
            ..
        } = self.state
        else {
            return vec![];
        };
        if peer != from || ringing != call_id {
            return vec![];
        }
        self.state = State::Idle;
        vec![CallAction::EventEnded {
            peer,
            call_id,
            reason: "answered_elsewhere",
        }]
    }

    /// Refus de notre offre : mêmes corrélations strictes que la réponse.
    pub(crate) fn on_decline(
        &mut self,
        from: [u8; 32],
        call_id: [u8; 16],
        reason: u8,
    ) -> Vec<CallAction> {
        let State::Outgoing {
            peer,
            call_id: ours,
            ..
        } = self.state
        else {
            return vec![];
        };
        if peer != from || ours != call_id {
            return vec![];
        }
        self.state = State::Idle;
        vec![CallAction::EventEnded {
            peer,
            call_id,
            reason: if reason == CALL_DECLINE_BUSY {
                "busy"
            } else {
                "declined"
            },
        }]
    }

    /// Fin d'appel émise par le pair : corrélation stricte (pair + `call_id`)
    /// sur chaque phase, sinon ignorée.
    pub(crate) fn on_hangup(&mut self, from: [u8; 32], call_id: [u8; 16]) -> Vec<CallAction> {
        match &self.state {
            State::Outgoing {
                peer,
                call_id: ours,
                ..
            } if *peer == from && *ours == call_id => {
                self.state = State::Idle;
                vec![CallAction::EventEnded {
                    peer: from,
                    call_id,
                    reason: "hangup",
                }]
            }
            State::Incoming {
                peer,
                call_id: ringing,
                ..
            } if *peer == from && *ringing == call_id => {
                self.state = State::Idle;
                vec![CallAction::EventEnded {
                    peer: from,
                    call_id,
                    reason: "canceled",
                }]
            }
            State::Active {
                peer,
                call_id: current,
                ..
            } if *peer == from && *current == call_id => {
                self.state = State::Idle;
                vec![
                    CallAction::LeaveAudio,
                    CallAction::EventEnded {
                        peer: from,
                        call_id,
                        reason: "hangup",
                    },
                ]
            }
            _ => vec![],
        }
    }

    /// Vivacité audio perdue pendant un appel actif (le pair a disparu) :
    /// l'appel se termine localement, **en silence**.
    ///
    /// 🔒 Aucun raccrochage émis, et c'est le point. Multi-appareil : si deux
    /// appareils de l'appelé décrochent dans le même aller-retour, l'appelant
    /// n'en honore qu'un — mais le perdant, lui, se croit en appel. Son audio
    /// ne vient jamais, et dix secondes plus tard il émettait un raccrochage
    /// vers le COMPTE de l'appelant. Or l'appelant ne peut pas distinguer ce
    /// raccrochage de celui du gagnant : la signalisation entrante est
    /// traduite en compte avant d'atteindre le moteur voix. Il coupait donc
    /// l'appel qu'il était en train de tenir.
    ///
    /// Ne rien émettre ne coûte rien dans le cas mono-appareil : si l'audio est
    /// perdu, c'est que le pair a disparu — le raccrochage n'arrivait déjà
    /// nulle part. Et si les deux sont vivants derrière une coupure réseau,
    /// chacun conclut de son côté au même délai. Le message ne servait que
    /// quand il ne servait à rien, et nuisait quand il servait.
    pub(crate) fn on_audio_lost(&mut self) -> Vec<CallAction> {
        let State::Active { peer, call_id, .. } = self.state else {
            return vec![];
        };
        self.state = State::Idle;
        vec![
            CallAction::LeaveAudio,
            CallAction::EventEnded {
                peer,
                call_id,
                reason: "lost",
            },
        ]
    }

    /// L'utilisateur rejoint un salon vocal de groupe : un appel ACTIF se
    /// termine (le salon prend la session audio) ; une sonnerie survit.
    pub(crate) fn on_room_takeover(&mut self) -> Vec<CallAction> {
        match self.state {
            State::Active { peer, call_id, .. } => {
                self.state = State::Idle;
                vec![
                    CallAction::SendHangup { to: peer, call_id },
                    CallAction::EventEnded {
                        peer,
                        call_id,
                        reason: "hangup",
                    },
                ]
            }
            _ => vec![],
        }
    }

    /// Passe d'horloge : timeout des sonneries et réémission de l'offre.
    pub(crate) fn tick(&mut self, now_ms: u64) -> Vec<CallAction> {
        match &mut self.state {
            State::Outgoing {
                peer,
                call_id,
                started_ms,
                last_offer_ms,
            } => {
                let (peer, call_id) = (*peer, *call_id);
                if now_ms.saturating_sub(*started_ms) >= RING_TIMEOUT_MS {
                    self.state = State::Idle;
                    return vec![
                        CallAction::SendHangup { to: peer, call_id },
                        CallAction::EventEnded {
                            peer,
                            call_id,
                            reason: "timeout",
                        },
                    ];
                }
                if now_ms.saturating_sub(*last_offer_ms) >= OFFER_RESEND_MS {
                    *last_offer_ms = now_ms;
                    return vec![CallAction::SendOffer { to: peer, call_id }];
                }
                vec![]
            }
            State::Incoming {
                peer,
                call_id,
                received_ms,
                last_offer_ms,
            } => {
                let (peer, call_id) = (*peer, *call_id);
                if now_ms.saturating_sub(*received_ms) >= RING_TIMEOUT_MS {
                    self.state = State::Idle;
                    return vec![CallAction::EventEnded {
                        peer,
                        call_id,
                        reason: "missed",
                    }];
                }
                // 🔒 Le filet : plus aucune offre depuis RING_STALE_MS. Soit
                // l'appelant a raccroché et l'annulation s'est perdue, soit il
                // a décroché ailleurs et le « décroché ailleurs » s'est perdu
                // aussi. Dans les deux cas la sonnerie n'a plus d'objet, et
                // l'attendre jusqu'au bout la ferait conclure « appel manqué »
                // pour un appel qui a été pris.
                if now_ms.saturating_sub(*last_offer_ms) >= RING_STALE_MS {
                    self.state = State::Idle;
                    return vec![CallAction::EventEnded {
                        peer,
                        call_id,
                        reason: "canceled",
                    }];
                }
                vec![]
            }
            State::Active {
                peer,
                call_id,
                taken_left,
                last_taken_ms,
                ..
            } => {
                if *taken_left == 0 || now_ms.saturating_sub(*last_taken_ms) < OFFER_RESEND_MS {
                    return vec![];
                }
                *taken_left -= 1;
                *last_taken_ms = now_ms;
                vec![CallAction::SendTaken {
                    to: *peer,
                    call_id: *call_id,
                }]
            }
            _ => vec![],
        }
    }

    /// Crée une nouvelle sonnerie entrante si la cadence du pair le permet
    /// (sinon : silence, aucune réponse — zéro amplification).
    fn try_new_ring(&mut self, from: [u8; 32], call_id: [u8; 16], now_ms: u64) -> Vec<CallAction> {
        if !Self::cadence_ok(
            &mut self.last_ring_ms,
            from,
            now_ms,
            NEW_RING_MIN_INTERVAL_MS,
        ) {
            return vec![];
        }
        self.state = State::Incoming {
            peer: from,
            call_id,
            received_ms: now_ms,
            last_offer_ms: now_ms,
        };
        vec![CallAction::EventIncoming {
            peer: from,
            call_id,
        }]
    }

    /// Réponse « occupé » bornée par pair (au plus une par fenêtre).
    fn busy_reply(&mut self, from: [u8; 32], call_id: [u8; 16], now_ms: u64) -> Vec<CallAction> {
        if !Self::cadence_ok(
            &mut self.last_busy_ms,
            from,
            now_ms,
            BUSY_REPLY_MIN_INTERVAL_MS,
        ) {
            return vec![];
        }
        vec![CallAction::SendDecline {
            to: from,
            call_id,
            reason: CALL_DECLINE_BUSY,
        }]
    }

    /// Vrai si l'action est due pour ce pair (et l'enregistre). Table bornée.
    fn cadence_ok(
        table: &mut HashMap<[u8; 32], u64>,
        peer: [u8; 32],
        now_ms: u64,
        min_interval_ms: u64,
    ) -> bool {
        if table.len() > PEER_TRACKING_MAX {
            table.clear();
        }
        match table.get(&peer) {
            Some(&last) if now_ms.saturating_sub(last) < min_interval_ms => false,
            _ => {
                table.insert(peer, now_ms);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accord_proto::core_msg::CALL_DECLINE_REJECTED;

    const ME: [u8; 32] = [0x50; 32];
    const ALICE: [u8; 32] = [0x10; 32]; // < ME : son appel gagne les croisés.
    const BOB: [u8; 32] = [0x90; 32]; // > ME : notre appel gagne les croisés.
    const CALL: [u8; 16] = [0xC1; 16];
    const CALL2: [u8; 16] = [0xC2; 16];

    fn machine() -> CallMachine {
        CallMachine::new(ME)
    }

    #[test]
    fn outgoing_call_rings_resends_and_times_out() {
        let mut m = machine();
        let actions = m.start(BOB, CALL, 0).unwrap();
        assert!(actions.contains(&CallAction::SendOffer {
            to: BOB,
            call_id: CALL
        }));
        assert!(actions.contains(&CallAction::EventOutgoing {
            peer: BOB,
            call_id: CALL
        }));
        assert_eq!(m.snapshot().phase, CallPhase::OutgoingRinging);

        // Un second appel simultané est refusé explicitement.
        assert!(m.start(ALICE, CALL2, 10).is_err());

        // Réémission de l'offre à cadence fixe.
        assert!(m.tick(OFFER_RESEND_MS - 1).is_empty());
        assert_eq!(
            m.tick(OFFER_RESEND_MS),
            vec![CallAction::SendOffer {
                to: BOB,
                call_id: CALL
            }]
        );

        // Timeout : raccrochage émis et événement de fin.
        let actions = m.tick(RING_TIMEOUT_MS);
        assert!(actions.contains(&CallAction::SendHangup {
            to: BOB,
            call_id: CALL
        }));
        assert!(actions.contains(&CallAction::EventEnded {
            peer: BOB,
            call_id: CALL,
            reason: "timeout",
        }));
        assert_eq!(m.snapshot().phase, CallPhase::Idle);
    }

    #[test]
    fn answer_connects_only_when_it_correlates() {
        let mut m = machine();
        m.start(BOB, CALL, 0).unwrap();
        // Réponse forgée : mauvais pair, mauvais call_id → ignorées.
        assert!(m.on_answer(ALICE, CALL, 10).is_empty());
        assert!(m.on_answer(BOB, CALL2, 10).is_empty());
        assert_eq!(m.snapshot().phase, CallPhase::OutgoingRinging);
        // Réponse corrélée : la session audio démarre.
        let actions = m.on_answer(BOB, CALL, 20);
        assert!(actions.contains(&CallAction::JoinAudio {
            peer: BOB,
            call_id: CALL
        }));
        assert_eq!(m.snapshot().phase, CallPhase::Active);
        // Rejouer la réponse une fois actif : sans effet.
        assert!(m.on_answer(BOB, CALL, 30).is_empty());
    }

    #[test]
    fn decline_and_busy_end_the_outgoing_call() {
        let mut m = machine();
        m.start(BOB, CALL, 0).unwrap();
        let actions = m.on_decline(BOB, CALL, CALL_DECLINE_REJECTED);
        assert_eq!(
            actions,
            vec![CallAction::EventEnded {
                peer: BOB,
                call_id: CALL,
                reason: "declined",
            }]
        );
        let mut m = machine();
        m.start(BOB, CALL, 0).unwrap();
        let actions = m.on_decline(BOB, CALL, CALL_DECLINE_BUSY);
        assert_eq!(
            actions[0],
            CallAction::EventEnded {
                peer: BOB,
                call_id: CALL,
                reason: "busy",
            }
        );
    }

    #[test]
    fn incoming_ring_accept_flow() {
        let mut m = machine();
        let actions = m.on_offer(ALICE, CALL, 0);
        assert_eq!(
            actions,
            vec![CallAction::EventIncoming {
                peer: ALICE,
                call_id: CALL
            }]
        );
        // Réémission de la même offre : dédupliquée, pas de nouvel événement.
        assert!(m.on_offer(ALICE, CALL, 500).is_empty());
        // Acceptation : réponse + session audio.
        let actions = m.accept(CALL, 1_000).unwrap();
        assert!(actions.contains(&CallAction::SendAnswer {
            to: ALICE,
            call_id: CALL
        }));
        assert!(actions.contains(&CallAction::JoinAudio {
            peer: ALICE,
            call_id: CALL
        }));
        assert_eq!(m.snapshot().phase, CallPhase::Active);
        // L'offre rejouée pendant l'appel actif réémet la réponse (perte).
        assert_eq!(
            m.on_offer(ALICE, CALL, 2_000),
            vec![CallAction::SendAnswer {
                to: ALICE,
                call_id: CALL
            }]
        );
    }

    #[test]
    fn incoming_ring_expires_as_missed() {
        // Un appel réellement non décroché : l'appelant sonne jusqu'au bout,
        // donc ses offres continuent d'arriver. C'est ce qui distingue
        // « manqué » de « annulé » — et c'est pourquoi le test doit les
        // rejouer : sans elles, le filet de RING_STALE_MS conclurait, à raison,
        // que plus personne n'appelle.
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        let mut t = 0;
        while t + OFFER_RESEND_MS < RING_TIMEOUT_MS {
            t += OFFER_RESEND_MS;
            m.on_offer(ALICE, CALL, t);
            assert!(m.tick(t).is_empty(), "sonnerie encore vivante à t={t}");
        }
        assert_eq!(
            m.tick(RING_TIMEOUT_MS),
            vec![CallAction::EventEnded {
                peer: ALICE,
                call_id: CALL,
                reason: "missed",
            }]
        );
    }

    #[test]
    fn ring_spam_is_rate_limited_and_replay_does_not_extend_deadline() {
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        m.decline(CALL).unwrap();
        // Nouvelle sonnerie immédiate du même pair : sous la cadence, muette.
        assert!(m.on_offer(ALICE, CALL2, 100).is_empty());
        assert_eq!(m.snapshot().phase, CallPhase::Idle);
        // Après la fenêtre de cadence, une nouvelle sonnerie repasse.
        let actions = m.on_offer(ALICE, CALL2, NEW_RING_MIN_INTERVAL_MS);
        assert_eq!(actions.len(), 1);
        // Rejouer la même offre n'étend jamais l'échéance de sonnerie.
        for t in (NEW_RING_MIN_INTERVAL_MS..RING_TIMEOUT_MS).step_by(1_000) {
            assert!(m.on_offer(ALICE, CALL2, t).is_empty());
        }
        let expiry = m.tick(NEW_RING_MIN_INTERVAL_MS + RING_TIMEOUT_MS);
        assert_eq!(
            expiry,
            vec![CallAction::EventEnded {
                peer: ALICE,
                call_id: CALL2,
                reason: "missed",
            }]
        );
    }

    #[test]
    fn busy_reply_is_sent_once_per_window_and_only_when_busy() {
        let mut m = machine();
        m.start(BOB, CALL, 0).unwrap();
        // Alice appelle pendant notre appel sortant : occupé (une fois).
        let actions = m.on_offer(ALICE, CALL2, 10);
        assert_eq!(
            actions,
            vec![CallAction::SendDecline {
                to: ALICE,
                call_id: CALL2,
                reason: CALL_DECLINE_BUSY,
            }]
        );
        // Réémissions dans la fenêtre : silence (pas d'amplification).
        assert!(m.on_offer(ALICE, CALL2, 500).is_empty());
        assert!(m.on_offer(ALICE, CALL2, 1_999).is_empty());
        // Fenêtre écoulée : une seule nouvelle réponse.
        assert_eq!(
            m.on_offer(ALICE, CALL2, 10 + BUSY_REPLY_MIN_INTERVAL_MS)
                .len(),
            1
        );
    }

    #[test]
    fn cross_calls_converge_deterministically() {
        // Alice (clé plus petite) et nous nous appelons mutuellement.
        // Côté nous : notre sortant vers Alice + offre d'Alice → son appel
        // gagne, on l'accepte automatiquement.
        let mut m = machine();
        m.start(ALICE, CALL, 0).unwrap();
        let actions = m.on_offer(ALICE, CALL2, 10);
        assert!(actions.contains(&CallAction::SendAnswer {
            to: ALICE,
            call_id: CALL2
        }));
        assert!(actions.contains(&CallAction::EventEnded {
            peer: ALICE,
            call_id: CALL,
            reason: "superseded",
        }));
        assert_eq!(m.snapshot().phase, CallPhase::Active);
        assert_eq!(m.snapshot().call_id, Some(CALL2));

        // Côté symétrique : sortant vers Bob (clé plus grande) + offre de
        // Bob → notre appel gagne, son offre est ignorée.
        let mut m = machine();
        m.start(BOB, CALL, 0).unwrap();
        assert!(m.on_offer(BOB, CALL2, 10).is_empty());
        assert_eq!(m.snapshot().phase, CallPhase::OutgoingRinging);
    }

    #[test]
    fn hangup_covers_every_phase_and_is_idempotent() {
        // Au repos : rien.
        assert!(machine().hangup().is_empty());
        // Sonnerie sortante : annulation.
        let mut m = machine();
        m.start(BOB, CALL, 0).unwrap();
        let actions = m.hangup();
        assert!(actions.contains(&CallAction::SendHangup {
            to: BOB,
            call_id: CALL
        }));
        // Sonnerie entrante : refus.
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        let actions = m.hangup();
        assert!(actions.contains(&CallAction::SendDecline {
            to: ALICE,
            call_id: CALL,
            reason: CALL_DECLINE_REJECTED,
        }));
        // Appel actif : raccrochage + sortie audio.
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        m.accept(CALL, 10).unwrap();
        let actions = m.hangup();
        assert!(actions.contains(&CallAction::LeaveAudio));
        assert_eq!(m.snapshot().phase, CallPhase::Idle);
    }

    #[test]
    fn peer_hangup_correlates_strictly() {
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        m.accept(CALL, 10).unwrap();
        // Forgé : mauvais pair ou mauvais call_id → ignoré.
        assert!(m.on_hangup(BOB, CALL).is_empty());
        assert!(m.on_hangup(ALICE, CALL2).is_empty());
        assert_eq!(m.snapshot().phase, CallPhase::Active);
        // Corrélé : l'appel se termine.
        let actions = m.on_hangup(ALICE, CALL);
        assert!(actions.contains(&CallAction::LeaveAudio));
        assert_eq!(m.snapshot().phase, CallPhase::Idle);
    }

    #[test]
    fn caller_cancel_replaces_stale_ring_with_new_offer() {
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        // L'annulation d'Alice s'est perdue ; elle rappelle avec un nouveau
        // call_id après la fenêtre de cadence : l'ancienne sonnerie se ferme,
        // la nouvelle s'ouvre.
        let actions = m.on_offer(ALICE, CALL2, NEW_RING_MIN_INTERVAL_MS + 1);
        assert_eq!(
            actions[0],
            CallAction::EventEnded {
                peer: ALICE,
                call_id: CALL,
                reason: "canceled",
            }
        );
        assert_eq!(
            actions[1],
            CallAction::EventIncoming {
                peer: ALICE,
                call_id: CALL2,
            }
        );
        assert_eq!(m.snapshot().call_id, Some(CALL2));
    }

    #[test]
    fn audio_loss_and_room_takeover_end_active_calls_only() {
        let mut m = machine();
        assert!(m.on_audio_lost().is_empty());
        assert!(m.on_room_takeover().is_empty());
        m.on_offer(ALICE, CALL, 0);
        // Une sonnerie survit à l'entrée dans un salon de groupe.
        assert!(m.on_room_takeover().is_empty());
        assert_eq!(m.snapshot().phase, CallPhase::IncomingRinging);
        m.accept(CALL, 10).unwrap();
        let actions = m.on_room_takeover();
        assert!(actions.contains(&CallAction::SendHangup {
            to: ALICE,
            call_id: CALL
        }));
        assert_eq!(m.snapshot().phase, CallPhase::Idle);

        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        m.accept(CALL, 10).unwrap();
        let actions = m.on_audio_lost();
        assert!(actions.contains(&CallAction::EventEnded {
            peer: ALICE,
            call_id: CALL,
            reason: "lost",
        }));
        // 🔒 Et surtout AUCUN raccrochage. Multi-appareil : l'appareil qui a
        // perdu la course au décrochage se croit en appel, son audio n'arrive
        // jamais, et un raccrochage de sa part serait indiscernable de celui du
        // gagnant côté appelant — qui couperait l'appel qu'il tient vraiment.
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, CallAction::SendHangup { .. })),
            "perdre l'audio ne doit rien émettre : {actions:?}"
        );
    }

    #[test]
    fn decrocher_previent_les_autres_appareils_de_lappele() {
        // L'appelant honore une réponse : il annonce « décroché » au COMPTE de
        // l'appelé, que la livraison éclate sur tous ses appareils.
        let mut m = machine();
        m.start(BOB, CALL, 0).unwrap();
        let actions = m.on_answer(BOB, CALL, 100);
        assert!(
            actions.contains(&CallAction::SendTaken {
                to: BOB,
                call_id: CALL
            }),
            "l'annonce doit partir avec la réponse honorée : {actions:?}"
        );

        // Et elle est réémise, parce qu'un datagramme se perd.
        let mut vues = 1;
        let mut t = 100;
        for _ in 0..10 {
            t += OFFER_RESEND_MS;
            if m.tick(t).contains(&CallAction::SendTaken {
                to: BOB,
                call_id: CALL,
            }) {
                vues += 1;
            }
        }
        assert_eq!(vues, TAKEN_RESENDS, "réémissions bornées, pas infinies");
    }

    #[test]
    fn decrocher_soi_meme_nannonce_rien() {
        // 🔒 Sens unique. Si l'appareil qui décroche annonçait lui aussi, il
        // enverrait le message au COMPTE DE L'APPELANT — dont les appareils ne
        // sonnent pas — et pas aux siens. Du bruit sans effet, et une fausse
        // impression que le cas est couvert.
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        let actions = m.accept(CALL, 10).unwrap();
        assert!(!actions
            .iter()
            .any(|a| matches!(a, CallAction::SendTaken { .. })));
        assert!(m.tick(10 + OFFER_RESEND_MS * 5).is_empty());
    }

    #[test]
    fn un_appareil_qui_sonne_seteint_sur_decroche_ailleurs() {
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        let actions = m.on_taken(ALICE, CALL);
        assert_eq!(
            actions,
            vec![CallAction::EventEnded {
                peer: ALICE,
                call_id: CALL,
                reason: "answered_elsewhere",
            }]
        );
        assert_eq!(m.snapshot().phase, CallPhase::Idle);
    }

    #[test]
    fn decroche_ailleurs_ne_touche_ni_le_gagnant_ni_un_autre_appel() {
        // Le gagnant reçoit le même message — il est adressé au compte — et
        // doit l'ignorer, sans quoi il raccrocherait l'appel qu'il vient de
        // prendre.
        let mut gagnant = machine();
        gagnant.on_offer(ALICE, CALL, 0);
        gagnant.accept(CALL, 10).unwrap();
        assert!(gagnant.on_taken(ALICE, CALL).is_empty());
        assert_eq!(gagnant.snapshot().phase, CallPhase::Active);

        // Et un message qui ne corrèle pas ne peut pas éteindre une sonnerie :
        // ni d'un autre pair, ni pour un autre appel.
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        assert!(m.on_taken(BOB, CALL).is_empty());
        assert!(m.on_taken(ALICE, CALL2).is_empty());
        assert_eq!(m.snapshot().phase, CallPhase::IncomingRinging);
    }

    #[test]
    fn une_sonnerie_sans_nouvelles_seteint_delle_meme() {
        // 🔒 Le filet, et le cœur de la tâche : il ne dépend d'AUCUN message
        // reçu. Si « décroché ailleurs » se perd entièrement, l'appareil
        // constate que l'appelant a cessé de réémettre son offre et conclut.
        // Sans lui, il sonnerait 45 s puis afficherait « appel manqué » pour un
        // appel qui a été pris.
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);

        // Tant que les offres arrivent, ça sonne.
        let mut t = 0;
        for _ in 0..10 {
            t += OFFER_RESEND_MS;
            m.on_offer(ALICE, CALL, t);
            assert!(m.tick(t).is_empty(), "sonnerie vivante à t={t}");
        }

        // Les offres s'arrêtent : l'appelant a décroché ailleurs.
        assert!(m.tick(t + RING_STALE_MS - 1).is_empty());
        assert_eq!(
            m.tick(t + RING_STALE_MS),
            vec![CallAction::EventEnded {
                peer: ALICE,
                call_id: CALL,
                reason: "canceled",
            }],
            "et surtout PAS « missed »"
        );
        assert_eq!(m.snapshot().phase, CallPhase::Idle);
    }

    #[test]
    fn rejouer_une_offre_ne_prolonge_pas_la_sonnerie() {
        // 🔒 L'invariant que le filet ne doit pas casser : `received_ms` fixe
        // l'échéance et ne bouge jamais. Sinon un appelant qui réémet sans fin
        // ferait sonner un appareil indéfiniment.
        let mut m = machine();
        m.on_offer(ALICE, CALL, 0);
        let mut t = 0;
        while t < RING_TIMEOUT_MS {
            t += OFFER_RESEND_MS;
            m.on_offer(ALICE, CALL, t);
            m.tick(t);
        }
        assert_eq!(
            m.snapshot().phase,
            CallPhase::Idle,
            "la sonnerie doit expirer malgré les réémissions"
        );
    }
}
