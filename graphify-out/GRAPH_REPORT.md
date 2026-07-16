# Graph Report - accord  (2026-07-16)

## Corpus Check
- 400 files · ~579,198 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 5857 nodes · 16691 edges · 212 communities (192 shown, 20 thin omitted)
- Extraction: 98% EXTRACTED · 2% INFERRED · 0% AMBIGUOUS · INFERRED: 286 edges (avg confidence: 0.79)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `05c4c04b`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- api.ts
- groups.ts
- useT
- useGroups
- Api
- NodeError
- MessageList.tsx
- state.rs
- server.rs
- Transfer
- AppShell.tsx
- MessageInput.tsx
- helpers.rs
- profile.rs
- network.rs
- lireFichier
- Endpoint
- Result
- DecodeError
- NodeInfo
- core_msg.rs
- maintenance.rs
- ProfilePopover.tsx
- NodeId
- MarkdownText.tsx
- Node
- Engine
- compressEmojiImage.ts
- CoreMsg
- ui.ts
- session.ts
- search.rs
- io.rs
- now_ms
- files.rs
- Node
- tcp.rs
- Runtime
- SimNet
- Changelog
- roundtrip.rs
- profile.rs
- session.rs
- ServerSoundsTab.tsx
- VoiceHandle
- engine.rs
- quickSwitch.ts
- EtatHote
- invite.rs
- DhtRecord
- Registry
- Identity
- relay_tunnel_e2e.rs
- handshake.rs
- CallMachine
- node_id_of
- discovery.rs
- room.rs
- voiceRecorder.ts
- CoreError
- mod.rs
- types.rs
- Suite communautaire : événements, stickers, avatar de serveur, bannière
- Db
- Roster
- lib.rs
- dms.test.ts
- bundle
- Result
- dsp.rs
- mod.rs
- Paths
- SocketAddr
- tests.rs
- frag.rs
- NAT traversal — conception (SPEC §11)
- rpc.ts
- compilerOptions
- friendcode.rs
- identity.rs
- relay.rs
- tests.rs
- hole_punch_e2e.rs
- etat.rs
- presence.rs
- voice.rs
- runtime.rs
- Writer
- codec.rs
- testnet.rs
- nat.rs
- Modals.tsx
- Node
- HardwareIo
- jitter.rs
- decorations-extra.tsx
- holepunch.rs
- node_with_channel
- bench_mesures.rs
- relay_e2e.rs
- decorations.tsx
- offline.rs
- hex.rs
- fichiers_e2e.rs
- relay.rs
- devDependencies
- .new
- IncomingInvite
- Db
- Manifest
- TransportDhtRpc
- VoiceMsg
- ChannelMsg
- CryptoError
- .route_file
- mod.rs
- ratelimit.rs
- Vad
- Distributing Accord
- folders.ts
- permissions
- mentions.rs
- Node
- calls_e2e.rs
- Methods
- dependencies
- fec.rs
- SPEC — Accord wire protocol, version 1
- params.rs
- service
- two_node_e2e.rs
- ManualClock
- endpoint.rs
- Suite vocale : appels 1-à-1, DSP de capture, modération vocale
- SECURITY — Accord's threat model
- RpcClient
- voice.test.ts
- crypt.rs
- RunningNode
- files.rs
- service_with_friend
- 2. Build and test
- 1. Rust — direct workspace dependencies
- ARCHITECTURE — Accord
- run
- friends.rs
- LossEstimator
- ErrorBoundary.tsx
- ProfilePersonalizationDemo.tsx
- mnemonic.rs
- seal
- Outbound
- boot_sim
- voice_e2e.rs
- README.md
- scripts
- audio.test.ts
- navPersistence.ts
- network_e2e.rs
- node_with_friend
- service_on_disk_with_group
- boot
- tcp_link_e2e.rs
- files.test.ts
- service_with_voice
- boot_sim
- boot_sim
- gain.rs
- Accord Threat Model — v0 accepted trade-offs
- Accord local API — JSON-RPC 2.0 over WebSocket
- automod.ts
- 2. Cryptography
- package.json
- notificationSound.test.ts
- ringtone.test.ts
- slashCommands.ts
- String
- 4. Attackers considered
- 6. CORE channel (0x02) — messaging, groups, presence
- bridge.test.ts
- ci.sh
- CookieJar
- main.rs
- build-linux.sh
- build-macos.sh
- eslint
- eslint-plugin-react-hooks
- eslint-plugin-react-refresh
- globals
- jsdom
- prettier
- @testing-library/jest-dom
- @testing-library/react
- @types/react-dom
- vitest
- preparer-code-source.sh
- .decode
- sound.ts
- .decode
- tailwindcss
- @types/react
- NoopSender

## God Nodes (most connected - your core abstractions)
1. `NodeError` - 333 edges
2. `CoreError` - 271 edges
3. `useT()` - 200 edges
4. `useUi` - 187 edges
5. `Runtime` - 132 edges
6. `Api` - 131 edges
7. `useGroups` - 106 edges
8. `interpolate()` - 96 edges
9. `NodeId` - 89 edges
10. `Identity` - 87 edges

## Surprising Connections (you probably didn't know these)
- `demarrer()` --references--> `Unlocked`  [EXTRACTED]
  app/src-tauri/src/commandes.rs → crates/accord-node/src/identity.rs
- `ErreurHote` --references--> `NodeError`  [EXTRACTED]
  app/src-tauri/src/erreur.rs → crates/accord-node/src/error.rs
- `CompteMeta` --references--> `AccountEntry`  [EXTRACTED]
  app/src-tauri/src/etat.rs → crates/accord-node/src/registry.rs
- `statut_du_coffre()` --references--> `Paths`  [EXTRACTED]
  app/src-tauri/src/etat.rs → crates/accord-node/src/identity.rs
- `CompteActif` --references--> `Paths`  [EXTRACTED]
  app/src-tauri/src/etat.rs → crates/accord-node/src/identity.rs

## Import Cycles
- 1-file cycle: `app/src-tauri/src/tray.rs -> app/src-tauri/src/tray.rs`
- 1-file cycle: `crates/accord-api/src/server.rs -> crates/accord-api/src/server.rs`
- 1-file cycle: `crates/accord-transport/src/tcp.rs -> crates/accord-transport/src/tcp.rs`
- 2-file cycle: `crates/accord-node/src/maintenance.rs -> crates/accord-node/src/runtime.rs -> crates/accord-node/src/maintenance.rs`
- 3-file cycle: `crates/accord-dht/src/lookup.rs -> crates/accord-dht/src/testnet.rs -> crates/accord-dht/src/node.rs -> crates/accord-dht/src/lookup.rs`

## Communities (212 total, 20 thin omitted)

### Community 0 - "api.ts"
Cohesion: 0.12
Nodes (22): PinnedPanel(), MentionInbox(), DM_ENTRY, inboxMock, markReadMock, viewOf(), HitRow(), MentionConversation (+14 more)

### Community 1 - "groups.ts"
Cohesion: 0.04
Nodes (56): PollCard(), PollCardProps, GroupSidebar(), GroupPoll, channelsByCategory(), handleMentionNodeEvent(), isChannelRestricted(), isChannelVisible() (+48 more)

### Community 2 - "useT"
Cohesion: 0.06
Nodes (61): SelectionBar(), CloseIcon(), EventForm(), toDatetimeLocalValue(), InviteEmbed(), RemoveFriendMenuIcon(), ConfirmButton(), messageOf() (+53 more)

### Community 3 - "useGroups"
Cohesion: 0.05
Nodes (88): message(), Avatar(), AvatarProps, deriveThreadName(), DmView(), GroupView(), MemberList(), mentionSet() (+80 more)

### Community 5 - "NodeError"
Cohesion: 0.07
Nodes (31): NodeError, Error, String, audit_pages_newest_first_with_cursor(), category_del_uncategorizes_channels(), category_edit_renames_and_validates(), channel_edit_moves_between_categories(), channel_perms_set_and_clear_override() (+23 more)

### Community 6 - "MessageList.tsx"
Cohesion: 0.05
Nodes (52): BodyText(), BodyTextProps, FORUM_POST_ROOT, ForumView(), ForumViewProps, forumChannel(), renderForum(), MessageActions() (+44 more)

### Community 7 - "state.rs"
Cohesion: 0.06
Nodes (82): add_channel_cannot_shadow_existing_thread(), add_poll_channel(), Applied, automod_set_words_rejects_oversized_list_and_spoofed_words_atomically(), automod_set_words_replaces_wholesale_normalizes_case_and_requires_manage_channels(), banned_member_cannot_rejoin_until_unban(), base_ops(), can_send_message_enforces_announcement_and_timeout() (+74 more)

### Community 8 - "server.rs"
Cohesion: 0.27
Nodes (17): auth_msg(), connect(), connection_limit_refuses_excess(), foreign_origin_is_rejected(), full_session_auth_call_and_notification(), malformed_json_yields_parse_error(), methods_require_authentication_first(), recv_json() (+9 more)

### Community 9 - "Transfer"
Cohesion: 0.05
Nodes (59): abandon_after_timeout_reports_final_progress(), Action, begin_is_bounded_and_rejects_duplicates(), Coordinator, echec_total_des_emissions_abandonne_rapidement(), emission_reussie_rearme_l_abandon_rapide(), Fetch, fixture() (+51 more)

### Community 10 - "AppShell.tsx"
Cohesion: 0.06
Nodes (52): AppShell(), handleCallEnded(), isViewingConversation(), maybePlayInviteSound(), maybePlaySound(), notifyEventStarted(), notifyNewMessage(), useGlobalShortcuts() (+44 more)

### Community 11 - "MessageInput.tsx"
Cohesion: 0.04
Nodes (66): AttachmentRow(), attendreTelechargementComplet(), CarteFichier(), Lightbox(), BASE_MS, lireMock, observerMock, saveMock (+58 more)

### Community 12 - "helpers.rs"
Cohesion: 0.05
Nodes (76): decode(), encode(), roundtrip_and_rejects(), Option, String, dispatch(), dm_messages_json(), BTreeSet (+68 more)

### Community 13 - "profile.rs"
Cohesion: 0.10
Nodes (66): db(), friend(), ingest_peer_profile(), invalid_peer_profile_from_friend_is_rejected_without_effect(), is_valid_decoration_id(), local_accent_color(), local_avatar(), local_avatar_decoration() (+58 more)

### Community 14 - "network.rs"
Cohesion: 0.05
Nodes (54): Active, adresse_externe_ipv4_et_ipv6(), etat_partage_ne_signale_que_les_transitions(), external_addr(), instantane_par_defaut_sans_mapping(), map_natpmp(), map_upnp(), NatError (+46 more)

### Community 15 - "lireFichier"
Cohesion: 0.19
Nodes (16): CustomEmoji(), CustomEmojiProps, aggregateReactions(), displayOf(), ReactionPill, ReactionRow(), ReactionRowProps, reactorsOf() (+8 more)

### Community 16 - "Endpoint"
Cohesion: 0.09
Nodes (26): AtomicU8, ClientCircuit, Endpoint, EndpointConfig, PeerLink, Pending, PendingOpen, Arc (+18 more)

### Community 17 - "Result"
Cohesion: 0.10
Nodes (33): attachments_roundtrip_ordered_and_wiped_on_delete(), Db, dm(), dm_lamport_lookup_and_latest_from_peer(), dm_message_getter_and_history_around_window(), dm_pins_add_list_remove_and_wiped_on_delete(), dm_raw(), dm_unread_counts_only_peer_messages_after_mark() (+25 more)

### Community 18 - "DecodeError"
Cohesion: 0.16
Nodes (14): decode_bool(), decode_profile_color(), Result, Self, Result, Self, Result, Self (+6 more)

### Community 19 - "NodeInfo"
Cohesion: 0.12
Nodes (31): find_node(), find_node_converges_on_target(), find_value(), find_value_bounded(), find_value_bounded_rejects_future_timestamp(), find_value_path(), find_value_prefers_path_consensus(), find_value_retrieves_stored_record() (+23 more)

### Community 20 - "core_msg.rs"
Cohesion: 0.08
Nodes (21): automod_set_words_rejects_oversized_list_oversized_word_and_truncation(), GroupOpBody, invite_ticket_signable_bytes(), invite_ticket_signable_bytes_are_stable_and_domain_separated(), malformed_new_ops_are_rejected(), MsgBody, pin_msg_body_roundtrips_and_rejects_malformed(), poll_msg_body_roundtrips_and_enforces_bounds() (+13 more)

### Community 21 - "maintenance.rs"
Cohesion: 0.08
Nodes (50): bootstrap_reconnect_tick(), cibles_de_resolution(), cibles_de_resolution_union_sans_doublon_et_bornee(), classer_candidat(), classer_candidat_local_vs_public(), coremsg_roundtrip_outbox(), decode_core(), decode_presence_value() (+42 more)

### Community 22 - "ProfilePopover.tsx"
Cohesion: 0.04
Nodes (69): ReplyBanner(), ThreadsListPanel(), BellOffMenuIcon(), buildNotifLevelItems(), CheckMenuIcon(), clamp(), ContextMenu(), CopyMenuIcon() (+61 more)

### Community 23 - "NodeId"
Cohesion: 0.08
Nodes (32): Distance, distance_symmetry_and_zero(), ordering_is_bitwise(), Fn, Option, Self, T, sort_by_distance() (+24 more)

### Community 24 - "MarkdownText.tsx"
Cohesion: 0.11
Nodes (17): CodeBlock(), BASH, CodeToken, CSS, highlightCode(), HTML, isTagPosition(), JS_TS (+9 more)

### Community 25 - "Node"
Cohesion: 0.09
Nodes (21): Node, Arc, Db, FnOnce, HashMap, HashSet, Into, Mutex (+13 more)

### Community 26 - "Engine"
Cohesion: 0.11
Nodes (17): Active, Engine, media_flags(), mod_flags_of(), ModFlags, HashMap, Option, Result (+9 more)

### Community 27 - "compressEmojiImage.ts"
Cohesion: 0.08
Nodes (35): AvatarCropper(), AvatarCropperProps, focusables(), FormeRecadreur, fichier, ajusterDimensions(), compresserStatique(), CompressOptions (+27 more)

### Community 28 - "CoreMsg"
Cohesion: 0.18
Nodes (47): announcement_channel_is_read_only_for_plain_members(), attachments_travel_and_persist_on_both_sides(), build_group(), channel_override_denying_send_blocks_composition(), channel_override_denying_send_or_view_blocks_compose(), check_slowmode(), check_thread_create_slowmode(), compose_group_delete() (+39 more)

### Community 29 - "ui.ts"
Cohesion: 0.08
Nodes (33): Lang, CibleProfil, clamp(), Density, FontScale, initialBool(), initialDensity(), initialEmojiSize() (+25 more)

### Community 30 - "session.ts"
Cohesion: 0.08
Nodes (42): PermissionRow(), SystemTab(), SelfProfile, accountCreate(), AccountMeta, accountRestore(), accountsList(), accountUnlock() (+34 more)

### Community 31 - "search.rs"
Cohesion: 0.10
Nodes (41): days_from_civil(), filters_are_extracted_and_words_kept(), hashed_tokens(), HasKind, index_and_search_intersection(), index_message(), index_stores_no_plaintext_tokens(), parse_iso_date() (+33 more)

### Community 32 - "io.rs"
Cohesion: 0.09
Nodes (35): audio_error(), chunker_emits_exact_frames(), downmix_i16(), f32_to_i16(), FrameChunker, LinearResampler, resampler_is_identity_at_equal_rates(), resampler_preserves_constant_signal() (+27 more)

### Community 33 - "now_ms"
Cohesion: 0.12
Nodes (22): attachments_json(), delivery_states_and_retry_reemits(), dm_delivery_state(), mark_read_sends_receipt_to_online_peer_once(), mark_read_stays_silent_for_offline_peer(), next_dm_of_kind(), Node, node_with_incoming_dm() (+14 more)

### Community 34 - "files.rs"
Cohesion: 0.12
Nodes (26): abandon_reporte_l_intention_selon_le_bareme_sans_la_supprimer(), abandon_terminal_supprime_apres_fetch_attempts_max(), adopter_borne_le_nombre_et_priorise_les_dues(), build(), build_intent(), cap_par_indice_borne_les_intentions_d_un_meme_pair(), Db, entry() (+18 more)

### Community 35 - "Node"
Cohesion: 0.14
Nodes (20): bit(), bitmap_pleine(), intention_de_telechargement_persistee_puis_soldee(), intention_survit_a_l_abandon_puis_reprend_a_la_reconnexion_du_pair(), media_auto_recupere_pose_un_plafond_persistant_sans_regresser_les_clics(), mime_par_extension(), Node, node_sur_disque() (+12 more)

### Community 36 - "tcp.rs"
Cohesion: 0.13
Nodes (33): addr_n(), admission_borne_globale_et_par_ip(), admit(), LinkHandle, longueur_nulle_ferme_le_lien(), mux_pair(), mux_route_sur_le_lien_tcp_et_remonte_les_trames(), mux_transmet_en_udp_sans_lien() (+25 more)

### Community 37 - "Runtime"
Cohesion: 0.09
Nodes (12): fixed_window_ok(), have_targets(), probe_pool_from_book(), AtomicUsize, HashSet, OnceLock, Vec, Runtime (+4 more)

### Community 38 - "SimNet"
Cohesion: 0.02
Nodes (101): callMock, friendsListMock, historyAroundMock, markReadMock, pinsMock, purgeMock, unpinMock, ForwardPicker() (+93 more)

### Community 39 - "Changelog"
Cohesion: 0.04
Nodes (46): [0.14.0] – [0.14.2] — 2026-07-12, [0.15.0] — 2026-07-12, [0.2.0] – [0.13.0], [1.0.0] — 2026-07-13, [1.0.1] — 2026-07-13, [1.0.2] — 2026-07-13, [1.1.0] — 2026-07-14, [1.2.0] — 2026-07-14 (+38 more)

### Community 40 - "roundtrip.rs"
Cohesion: 0.08
Nodes (32): Hello, Option, Result, Vec, tcp_deframe(), tcp_frame(), control_msgs_roundtrip(), core_msgs_roundtrip() (+24 more)

### Community 41 - "profile.rs"
Cohesion: 0.13
Nodes (28): addr(), bind_reuse_partage_le_meme_port(), bind_reuse_transmet_un_datagramme(), Fabric, nat_symetrique_croise_bloque_le_poinconnage(), nat_symetrique_mapping_par_destination_et_filtrage(), NatState, NetConditions (+20 more)

### Community 42 - "session.rs"
Cohesion: 0.12
Nodes (21): linked_pair(), needs_rekey_on_age(), next_epoch_key(), nonce_for(), old_window_replay_rejected(), RecvEpoch, rekey_epoch_transition(), reordering_within_window_accepted() (+13 more)

### Community 43 - "ServerSoundsTab.tsx"
Cohesion: 0.03
Nodes (72): SELF, voiceStatusMock, DeviceSelect(), IDLE_LEVEL, MicLevel, MicMeter(), readMicLevel(), devicesMock (+64 more)

### Community 44 - "VoiceHandle"
Cohesion: 0.10
Nodes (15): CallSnapshot, Option, Cmd, Option, Result, Sender, String, UnboundedSender (+7 more)

### Community 45 - "engine.rs"
Cohesion: 0.14
Nodes (37): CallPhase, audio_unavailable(), call_start_requires_friendship_and_rejects_self(), capture_is_sent_to_participants_and_gated_by_mute(), deafen_forces_mute_and_undeafen_restores_requested_state(), devices_are_empty_and_default_in_simulated_mode(), eventually(), eventually_phase() (+29 more)

### Community 46 - "quickSwitch.ts"
Cohesion: 0.09
Nodes (29): SearchIcon(), ItemIcon(), optionDomId(), QuickSwitcher(), ResultRow(), ServerInitialBadge(), SELF, ServerIcon() (+21 more)

### Community 47 - "EtatHote"
Cohesion: 0.06
Nodes (59): account_create(), account_restore(), account_unlock(), accounts_list(), app_quit(), create_identity(), demarrer(), en_arriere_plan() (+51 more)

### Community 48 - "invite.rs"
Cohesion: 0.13
Nodes (35): author_invite_create(), author_invite_create_with(), author_invite_create_with_supports_unlimited_and_never_expiring(), AuthoredInvite, b64url_decode(), b64url_encode(), b64url_val(), build_invite_ticket() (+27 more)

### Community 49 - "DhtRecord"
Cohesion: 0.12
Nodes (26): Inner, expiration_removes_records(), expiry_bounds_enforced(), future_timestamp_rejected(), identity_key_mismatch_rejected(), identity_publisher_mismatch_rejected(), identity_record(), identity_record_is_bound() (+18 more)

### Community 50 - "Registry"
Cohesion: 0.16
Nodes (20): AccountEntry, corrupt_registry_file_rebuilds_from_scan_without_data_loss(), create_read_update_roundtrip(), file_time_ms(), generate_id(), legacy_dir_auto_registered_exactly_once_idempotent_across_restarts(), most_recently_used_picks_the_highest_last_used_ms(), record_use_on_unknown_non_legacy_id_is_ignored_without_fabricating_an_entry() (+12 more)

### Community 51 - "Identity"
Cohesion: 0.18
Nodes (34): compose(), compose_delete(), compose_edit(), compose_persists_attachments_and_delete_wipes_them(), compose_persists_indexes_and_ack_confirms(), compose_pin(), compose_reaction(), compose_read_receipt() (+26 more)

### Community 52 - "relay_tunnel_e2e.rs"
Cohesion: 0.21
Nodes (29): config(), connect_until(), establish(), establish_tunnel(), faille_a_tunnel_ignore_hello_reinjecte_d_identite_non_liee(), faille_b_keepalive_tunnele_est_enveloppe_et_maintient_la_session(), faille_b_session_tunnelee_expiree_nettoie_le_circuit_sans_paniquer(), faille_c_bis_hello_invalide_ne_reserve_aucun_slot_et_le_legitime_passe() (+21 more)

### Community 53 - "handshake.rs"
Cohesion: 0.22
Nodes (21): cookie_roundtrip_and_rotation(), derive_keys(), expected_identity_binding_accepts_real_peer(), full_handshake_derives_same_keys(), Initiator, insufficient_pow_rejected(), NonceCache, pair() (+13 more)

### Community 54 - "CallMachine"
Cohesion: 0.18
Nodes (20): answer_connects_only_when_it_correlates(), audio_loss_and_room_takeover_end_active_calls_only(), busy_reply_is_sent_once_per_window_and_only_when_busy(), CallAction, caller_cancel_replaces_stale_ring_with_new_offer(), CallMachine, cross_calls_converge_deterministically(), decline_and_busy_end_the_outgoing_call() (+12 more)

### Community 55 - "node_id_of"
Cohesion: 0.17
Nodes (34): block(), blocked_peer_is_silently_ignored(), cle(), crossed_requests_auto_accept(), debit_ingestion_rejette_une_rafale_dans_la_meme_fenetre(), display_names_are_validated(), friend_request_rate_ok(), identity_record() (+26 more)

### Community 56 - "discovery.rs"
Cohesion: 0.11
Nodes (30): announce(), consume(), host_name(), instance_name(), LanShared, LanSink, nom_instance_et_hote_reversibles(), parse_peer() (+22 more)

### Community 57 - "room.rs"
Cohesion: 0.12
Nodes (26): CodecError, String, boosted_gain_saturates_at_i16_bounds(), capture_dsp_agc_applies_before_vad_and_encoding(), capture_gates_on_vad_and_sequences(), deafened_room_drains_without_decoding_or_playing(), ducking_attenuates_playback_and_is_bounded(), end_to_end_capture_transport_playback() (+18 more)

### Community 58 - "voiceRecorder.ts"
Cohesion: 0.09
Nodes (14): { FakeVoiceRecorder }, CANDIDATE_MIME_TYPES, extensionForMime(), pickAudioMimeType(), FakeMediaRecorder, FakeStream, FakeTrack, installBrowserMocks() (+6 more)

### Community 59 - "CoreError"
Cohesion: 0.14
Nodes (17): counts_and_mark_read(), Db, delete_mention_removes_entry(), entry(), group_entry_roundtrips_channel(), inbox_orders_recent_first_and_paginates(), mention_raw(), MentionEntry (+9 more)

### Community 60 - "mod.rs"
Cohesion: 0.18
Nodes (42): accept_sealed_key(), apply_moderation(), author_op(), author_op_refuses_unauthorized_action(), craft_free_id_op(), create_concurrente_ne_bascule_pas_un_groupe_moderne_en_regime_herite(), create_group(), create_then_author_ops_builds_consistent_state() (+34 more)

### Community 61 - "types.rs"
Cohesion: 0.18
Nodes (15): node(), rejects_forged_node_id(), rejects_no_address(), DhtMessage, FileMsg, ChannelMsg, ControlMsg, RelayMsg (+7 more)

### Community 62 - "Suite communautaire : événements, stickers, avatar de serveur, bannière"
Cohesion: 0.06
Nodes (32): 1.1 Méthodes RPC, 1.2 Événement WebSocket : `event.group_event_started`, 1.3 Forme dans `groups.state`, 1. Événements planifiés (`groups.events.*`), 2.1 Méthodes RPC, 2.2 Envoi d'un sticker (`groups.send` étendu), 2. Stickers de serveur (`groups.stickers.*`), 3.1 Méthode RPC (+24 more)

### Community 63 - "Db"
Cohesion: 0.12
Nodes (14): Db, group_keys_by_epoch(), local_membership_defaults_to_none_and_roundtrips(), LocalMembership, op(), oplog_orders_by_lamport_then_author(), prune_slowmode_drops_deleted_channels_and_departed_members_only(), BTreeSet (+6 more)

### Community 64 - "Roster"
Cohesion: 0.13
Nodes (17): force_silent_closes_indicator(), leave_removes_and_reports_presence(), mute_state_transitions_are_reported_once(), PeerState, pk(), rapid_mute_flips_are_throttled_but_state_stays_current(), Roster, RosterEvent (+9 more)

### Community 65 - "lib.rs"
Cohesion: 0.10
Nodes (31): bind_candidates(), bind_p2p(), bind_tcp_listener(), candidate_ports(), default_bootstrap_env(), NodeConfig, parse_default_bootstrap(), parse_default_bootstrap_filtre_et_borne() (+23 more)

### Community 66 - "dms.test.ts"
Cohesion: 0.10
Nodes (24): fetchDmPage(), fetchGroupPage(), mergeById(), mergeOlderPage(), mergeRecentPage(), MergeResult, RpcCaller, Sequenced (+16 more)

### Community 67 - "bundle"
Cohesion: 0.07
Nodes (28): app, security, windows, build, beforeBuildCommand, beforeDevCommand, devUrl, frontendDist (+20 more)

### Community 68 - "Result"
Cohesion: 0.15
Nodes (16): build(), Contact, contact_note_set_get_update_and_clear(), ContactState, Db, row_to_contact(), Option, Result (+8 more)

### Community 69 - "dsp.rs"
Cohesion: 0.16
Nodes (18): Agc, agc_attenuates_loud_input_quickly(), agc_boosts_quiet_speech_toward_target(), agc_gain_is_bounded_and_silence_does_not_adapt(), CaptureDsp, cpu_cost_measurement(), Denoiser, denoiser_attenuates_stationary_white_noise() (+10 more)

### Community 70 - "mod.rs"
Cohesion: 0.16
Nodes (16): Connection, Db, db_file_is_not_plaintext(), hex_key(), key(), lamport_is_monotonic_and_merges_observed(), migration_marks_pre_existing_groups_as_joined(), migration_v7_conserve_les_intentions_de_telechargement_existantes() (+8 more)

### Community 71 - "Paths"
Cohesion: 0.22
Nodes (20): derive_db_key(), create(), create_refuses_when_identity_exists(), create_seals_and_unlock_recovers_same_identity(), create_with_phrase(), Paths, recovery_phrase_restores_the_same_identity(), restore_from_phrase() (+12 more)

### Community 72 - "SocketAddr"
Cohesion: 0.13
Nodes (6): direct_target(), NetSignal, Duration, NetworkStatus, Result, SocketAddr

### Community 73 - "tests.rs"
Cohesion: 0.05
Nodes (78): avatar_publication_goes_through_files_subsystem(), avatar_removal_clears_hash_and_reannounces(), banner_publication_goes_through_files_subsystem(), banner_removal_clears_hash_and_reannounces(), bio_set_clear_and_self_profile_shape(), ingested_friend_profile_persists_bio_avatar_and_banner(), next_profile(), Node (+70 more)

### Community 74 - "frag.rs"
Cohesion: 0.16
Nodes (21): borne_de_messages_simultanes(), borne_memoire_de_reassemblage(), doublon_est_ignore_sans_erreur(), frame(), gros_message_est_fragmente_et_reassemble(), indices_invalides_rejetes(), message_a_la_limite_reste_un_cadre_unique(), Partial (+13 more)

### Community 75 - "NAT traversal — conception (SPEC §11)"
Cohesion: 0.07
Nodes (26): 1. Diagnostic, 2. Conception retenue, 3. Preuve reproductible, 3bis. Durcissement post-revue de sécurité (H1, M1), 3ter. « Code ami introuvable sur le réseau » — rendez-vous partagé (défaut), 4. Limites connues (documentées, non masquées), Correctifs, D1 — Les tables de routage DHT n'apprennent jamais personne (cause racine) (+18 more)

### Community 76 - "rpc.ts"
Cohesion: 0.10
Nodes (10): EventHandler, Pending, RpcCallError, RpcError, RpcStatus, StatusHandler, connectReady(), FakeWs (+2 more)

### Community 77 - "compilerOptions"
Cohesion: 0.07
Nodes (26): compilerOptions, allowImportingTsExtensions, exactOptionalPropertyTypes, isolatedModules, jsx, lib, module, moduleDetection (+18 more)

### Community 78 - "friendcode.rs"
Cohesion: 0.16
Nodes (18): checksum(), deep_link(), deep_link_roundtrip(), dht_key_stable_and_distinct(), display_parse_roundtrip(), format_shape(), FriendCode, from_payload_roundtrips_through_display() (+10 more)

### Community 79 - "identity.rs"
Cohesion: 0.14
Nodes (15): compute_pow(), ed25519_rfc8032_test_vector_1(), hex_literal(), Identity, identity_deterministic_from_seed(), leading_zero_bits(), pow_generate_and_verify(), Result (+7 more)

### Community 80 - "relay.rs"
Cohesion: 0.13
Nodes (25): addr(), classify_nat(), fusion_candidats_pair_puis_domicile_sans_doublon_et_bornee(), is_relay_candidate(), merge_relay_candidates(), nat_kind_consensus_donne_cone(), nat_kind_divergence_donne_symmetric(), nat_kind_trop_peu_observations_reste_unknown() (+17 more)

### Community 81 - "tests.rs"
Cohesion: 0.15
Nodes (24): group_audit_lists_decoded_entries_newest_first(), group_automod_set_get_normalizes_case_and_reflects_in_state(), group_automod_set_validates_at_boundary(), group_categories_edit_delete_and_channel_move(), group_channel_perms_override_and_scoped_my_permissions(), group_channel_slowmode_defaults_off_and_set_reflects_in_state(), group_channel_slowmode_validates_at_boundary(), group_channels_add_voice_edit_delete_and_categories() (+16 more)

### Community 82 - "hole_punch_e2e.rs"
Cohesion: 0.18
Nodes (22): DatagramSocket, Send, Sync, build_node(), config(), CountingSocket, hole_punch(), mutual_punch_etablit_une_seule_session_bidirectionnelle() (+14 more)

### Community 83 - "etat.rs"
Cohesion: 0.17
Nodes (17): Echo, Send, Sync, Service, CodeResolver, dispatch(), node_error_to_rpc(), NodeService (+9 more)

### Community 84 - "presence.rs"
Cohesion: 0.14
Nodes (15): custom_status_is_validated_before_any_write(), db(), own_presence(), own_presence_defaults_to_online_without_custom_text(), OwnStatus, Db, Option, Result (+7 more)

### Community 85 - "voice.rs"
Cohesion: 0.20
Nodes (14): flag_bytes(), Node, peer_volume_key(), read_device_meta(), read_flag_meta(), read_volume_meta(), Db, Option (+6 more)

### Community 86 - "runtime.rs"
Cohesion: 0.13
Nodes (19): AddressBook, assemble_presence_addrs(), discover_local_ips(), evict_oldest_if_full(), eviction_lru_nop_si_cle_deja_presente_ou_table_non_pleine(), eviction_lru_retire_le_plus_ancien_et_preserve_les_autres(), local_addrs(), presence_addrs_borne_a_max_et_priorise_observee() (+11 more)

### Community 88 - "codec.rs"
Cohesion: 0.15
Nodes (13): AudioCodec, OpusCodec, passthrough_roundtrips_pcm(), PassthroughCodec, pcm8_frame_fits_wire_limit_and_roundtrips_coarsely(), Pcm8Codec, Option, Result (+5 more)

### Community 89 - "testnet.rs"
Cohesion: 0.22
Nodes (14): DhtRpc, Send, Sync, expired_records_are_purged_everywhere(), network_populates_routing_tables(), node_get_finds_published_record(), rate_limiter_silently_drops_floods(), Arc (+6 more)

### Community 90 - "nat.rs"
Cohesion: 0.22
Nodes (17): addr(), Candidate, candidate_ordering(), CandidateKind, consensus_needs_two_distinct_peers(), divergent_observations_flag_symmetric(), ObservedAddrs, ordered_candidates() (+9 more)

### Community 91 - "Modals.tsx"
Cohesion: 0.04
Nodes (92): App(), AddFriend(), JoinServerForm(), MarkdownText(), ChannelKindOption(), CreateChannelModal(), CreateGroupModal(), CreatePollModal() (+84 more)

### Community 92 - "Node"
Cohesion: 0.18
Nodes (12): contact_note_roundtrips_trims_and_bounds(), dm_everyone_triggers_without_a_local_name(), dm_mention_by_name_records_inbox_entry(), Node, node_with_friend(), role_names_of(), MentionEntry, Option (+4 more)

### Community 93 - "HardwareIo"
Cohesion: 0.17
Nodes (17): MicTest, Self, audio_thread(), HardwareIo, mic_test_thread(), MicCapture, Arc, AtomicBool (+9 more)

### Community 94 - "jitter.rs"
Cohesion: 0.19
Nodes (14): JitterBuffer, late_frame_is_dropped(), missing_frame_yields_conceal(), pkt(), Playout, primes_then_plays_in_order(), reorders_out_of_order_arrivals(), BTreeMap (+6 more)

### Community 96 - "holepunch.rs"
Cohesion: 0.16
Nodes (12): addr(), CoordState, etat_borne_meme_sous_un_flot_d_identites(), Outstanding, PunchCoordinator, HashMap, Mutex, SocketAddr (+4 more)

### Community 97 - "node_with_channel"
Cohesion: 0.15
Nodes (8): DhtConfig, KademliaNode, Default, Mutex, Option, R, Self, Vec

### Community 98 - "bench_mesures.rs"
Cohesion: 0.16
Nodes (17): befriend(), bench_debit_dm(), bench_latence_voix(), bench_lookup_dht(), boot(), eventually(), ms(), percentile() (+9 more)

### Community 99 - "relay_e2e.rs"
Cohesion: 0.24
Nodes (20): config(), data(), establish(), Node, open(), recv_accepted(), recv_rejected(), recv_relayed_blob() (+12 more)

### Community 100 - "decorations.tsx"
Cohesion: 0.10
Nodes (7): BLOSSOMS, DECORATION_BY_ID, DECORATION_UI_TEXT, decorationById(), DecorationLabel, EFFECT_BY_ID, LAUREL_LEAVES

### Community 101 - "offline.rs"
Cohesion: 0.23
Nodes (17): bundle(), day_of_ms(), deposit_fragments_and_reassembles(), deposit_from_unexpected_sender_is_rejected(), deposit_records(), envelope_cannot_be_redirected(), envelope_signs_then_seals(), fragment_total() (+9 more)

### Community 102 - "hex.rs"
Cohesion: 0.38
Nodes (6): dispatch(), mention_entry_json(), MentionEntry, Node, Result, Value

### Community 103 - "fichiers_e2e.rs"
Cohesion: 0.16
Nodes (19): appel(), attendre_fin_de_transfert(), base64(), boot(), lecture_d_un_fichier_absent_declenche_le_telechargement(), lecture_en_ligne_refusee_au_dela_de_8_mio(), partage_lecture_statut_et_sauvegarde_locaux(), Arc (+11 more)

### Community 104 - "relay.rs"
Cohesion: 0.17
Nodes (12): bandwidth_cap_enforced_and_resets(), Circuit, circuit_cap_enforced(), close_by_requires_endpoint_provenance(), close_removes_circuit(), node(), open_forward_both_directions(), RelayDecision (+4 more)

### Community 105 - "devDependencies"
Cohesion: 0.10
Nodes (21): devDependencies, autoprefixer, postcss, @tauri-apps/cli, @testing-library/jest-dom, @testing-library/react, @testing-library/user-event, typescript (+13 more)

### Community 106 - ".new"
Cohesion: 0.27
Nodes (10): responses_serialize_exclusively(), RpcError, RpcNotification, RpcRequest, RpcResponse, Into, Option, Self (+2 more)

### Community 107 - "IncomingInvite"
Cohesion: 0.21
Nodes (14): Db, IncomingInvite, insert_get_list_and_remove_roundtrip(), insert_incoming_invite_enforces_per_inviter_cap(), multiple_invites_are_independent(), raw_row(), Option, Result (+6 more)

### Community 108 - "Db"
Cohesion: 0.13
Nodes (11): Db, destinations_distinctes_ordonnees_et_bornees(), expiry_purges_old_items(), OutboxItem, queue_lifecycle(), Result, Vec, Db (+3 more)

### Community 109 - "Manifest"
Cohesion: 0.17
Nodes (17): accept_loop(), authenticate(), dispatch(), handle_connection(), Arc, Duration, Error, Receiver (+9 more)

### Community 110 - "TransportDhtRpc"
Cohesion: 0.13
Nodes (5): Arc, Receiver, Self, UnboundedReceiver, tcp_punch_toward()

### Community 111 - "VoiceMsg"
Cohesion: 0.19
Nodes (8): AuthToken, debug_never_prints_token(), Debug, Formatter, Result, Self, String, tokens_are_unique_and_verify()

### Community 112 - "ChannelMsg"
Cohesion: 0.25
Nodes (16): bound_send_to_expected_identity_succeeds(), config(), gros_bloc(), gros_message_fragmente_et_reassemble(), gros_message_perdu_partiellement_echoue_puis_reussit_a_la_reemission(), message_survives_packet_loss(), Node, node_announce_flood_borne_par_session() (+8 more)

### Community 113 - "CryptoError"
Cohesion: 0.21
Nodes (13): derive_search_key(), corrupt_vault_rejected(), derive_key(), open_vault(), Default, Result, Self, Vec (+5 more)

### Community 114 - ".route_file"
Cohesion: 0.19
Nodes (18): correspond(), correspondRecent(), EmojiPicker(), EmojiPickerProps, labelCategorie(), EmojiPick, EMOJIS_UNICODE, EmojiUnicode (+10 more)

### Community 115 - "mod.rs"
Cohesion: 0.26
Nodes (14): analyserFragment(), analyserMarkdown(), construireListe(), contenuBloc(), estBord(), finDeLigne(), finDeParagraphe(), lireBlocCode() (+6 more)

### Community 116 - "ratelimit.rs"
Cohesion: 0.22
Nodes (10): Bucket, burst_then_throttle(), expensive_rpc_costs_more(), ip(), per_ip_isolation(), RateLimiter, refill_caps_at_capacity(), HashMap (+2 more)

### Community 117 - "Vad"
Cohesion: 0.19
Nodes (10): dbfs_scale_is_sane(), hangover_keeps_gate_open_briefly(), loud_frame_is_active(), rms_is_normalized_between_zero_and_one(), Default, Self, Vec, silence_is_inactive() (+2 more)

### Community 118 - "Distributing Accord"
Cohesion: 0.11
Nodes (17): 1. Source code (any OS), 2. macOS (run on a Mac), 3. Windows (run on Windows, in PowerShell), 4. Linux (run on Linux), 5. Everything via CI (recommended for Windows + Linux), Common to all application builds, Distributing Accord, Distribution folder structure (+9 more)

### Community 119 - "folders.ts"
Cohesion: 0.22
Nodes (10): addServerToFolder(), createFolder(), deleteFolder(), folderOfServer(), FoldersState, parseFolders(), removeServerFromFolders(), renameFolder() (+2 more)

### Community 120 - "permissions"
Cohesion: 0.12
Nodes (15): description, identifier, permissions, $schema, windows, autostart:allow-disable, autostart:allow-enable, autostart:allow-is-enabled (+7 more)

### Community 121 - "mentions.rs"
Cohesion: 0.25
Nodes (15): contains_mention(), detect(), everyone_and_here_both_trigger(), matches_display_name_case_insensitively(), matches_friend_code(), matches_role_names(), me(), MentionSelf (+7 more)

### Community 122 - "Node"
Cohesion: 0.15
Nodes (17): friend_remove_drops_contact_and_notifies_peer(), friend_remove_keeps_dm_history(), ingested_friend_remove_drops_friendship_only(), invisible_broadcasts_offline_without_custom_text(), next_core(), Node, node_with_friend(), own_presence_persists_and_broadcasts_status() (+9 more)

### Community 123 - "calls_e2e.rs"
Cohesion: 0.18
Nodes (15): befriended_pair(), boot(), eventually(), one_to_one_call_rings_connects_carries_audio_and_hangs_up(), F, Fn, Option, Path (+7 more)

### Community 124 - "Methods"
Cohesion: 0.13
Nodes (15): Attachments, Direct messaging, Files, Friends, Groups, Identity, Mentions, Methods (+7 more)

### Community 125 - "dependencies"
Cohesion: 0.13
Nodes (15): dependencies, react, react-dom, @tauri-apps/api, @tauri-apps/plugin-autostart, @tauri-apps/plugin-dialog, @tauri-apps/plugin-notification, zustand (+7 more)

### Community 126 - "fec.rs"
Cohesion: 0.29
Nodes (12): blocks(), coder(), full_group_survives_four_losses(), parity_for_group(), partial_group_with_uneven_tail_reconstructs(), reconstruct_group(), Option, Result (+4 more)

### Community 127 - "SPEC — Accord wire protocol, version 1"
Cohesion: 0.13
Nodes (15): 0.1 Version compatibility, 0. Encoding conventions (accord-proto), 10. Relay (channel 0x05), 11. NAT traversal, 12. Protocol error codes, 13.1 Session fragmentation (encrypted transport), 13. Numeric limits (decode guardrails), 1. Outer envelope (UDP datagram / TCP frame) (+7 more)

### Community 128 - "params.rs"
Cohesion: 0.60
Nodes (3): adapt(), healthy_network_ramps_up_by_steps_capped(), output_is_always_within_bounds()

### Community 129 - "service"
Cohesion: 0.14
Nodes (14): dm_requires_friend(), group_create_and_state_via_api(), group_history_around_centers_on_target(), group_history_renders_exact_text_shape(), identity_self_returns_profile(), invalid_params_are_rejected_at_boundary(), profile_get_set_exact_shapes(), profile_set_avatar_validates_at_the_boundary() (+6 more)

### Community 130 - "two_node_e2e.rs"
Cohesion: 0.24
Nodes (13): boot(), contact_name(), dm_de_20000_caracteres_traverse_le_reseau(), eventually(), profile_names_exchange_on_friendship_and_propagate(), FnMut, Option, Path (+5 more)

### Community 131 - "ManualClock"
Cohesion: 0.18
Nodes (8): Clock, ManualClock, Arc, AtomicU64, Self, Send, Sync, SystemClock

### Community 132 - "endpoint.rs"
Cohesion: 0.27
Nodes (7): CryptoError, check_freshness(), Established, Debug, Formatter, Result, verify_signature()

### Community 133 - "Suite vocale : appels 1-à-1, DSP de capture, modération vocale"
Cohesion: 0.14
Nodes (13): 1.1 Méthodes RPC, 1.2 Événements, 1.3 États et transitions, 1.4 `voice.status` pendant un appel, 1. Appels 1-à-1 (`calls.*`), 2. Qualité audio : suppression de bruit et AGC, 3.1 Méthode RPC, 3.2 Événements et état (+5 more)

### Community 134 - "SECURITY — Accord's threat model"
Cohesion: 0.14
Nodes (14): 1. Principles, 2. Primitives and implementations, 3.1 Encrypted and authenticated transport, 3.2 Anti-Sybil and DHT, 3.3 End-to-end contents, 3.4 Data at rest (disk theft), 3.5 Local surface (UI ↔ node), 3. Guarantees offered (+6 more)

### Community 136 - "voice.test.ts"
Cohesion: 0.28
Nodes (6): ApiServer, NotificationHub, Default, Drop, Sender, JoinHandle

### Community 137 - "crypt.rs"
Cohesion: 0.33
Nodes (12): aad(), decrypt_group_msg(), encrypt_group_msg(), generate_group_key(), key_roundtrips_through_sealed_box(), message_roundtrips_and_context_is_bound(), open_group_key(), Result (+4 more)

### Community 138 - "RunningNode"
Cohesion: 0.42
Nodes (8): link_pair(), Node, octets_forges_sur_tcp_ne_paniquent_pas_l_endpoint(), Arc, SocketAddr, UnboundedReceiver, session_complete_sur_lien_tcp(), spawn_mux_node()

### Community 139 - "files.rs"
Cohesion: 0.26
Nodes (10): base64(), blocs_detenus(), dispatch(), param_indice(), param_racine(), Node, Option, Result (+2 more)

### Community 140 - "service_with_friend"
Cohesion: 0.15
Nodes (13): attachments_are_validated_at_boundary(), dm_delete_renders_unknown_body_and_tombstone(), dm_edit_fills_edited_and_keeps_original_body(), dm_history_renders_exact_text_shape(), dm_history_renders_reply_to_when_set(), dm_mark_read_ok_and_friends_list_has_presence_and_unread(), dm_pins_and_history_around_flow(), dm_react_adds_then_removes_reaction() (+5 more)

### Community 141 - "2. Build and test"
Cohesion: 0.15
Nodes (12): 1. Repository structure, 2. Build and test, 3. Cargo features, 4. Test harness, 5. Project conventions, All at once: `./ci.sh`, Crate dependency graph, Desktop application (Tauri) (+4 more)

### Community 142 - "1. Rust — direct workspace dependencies"
Cohesion: 0.15
Nodes (12): 1. Rust — direct workspace dependencies, 2. JavaScript / TypeScript — direct dependencies (`app/package.json`), 3. Points of attention, Apache-2.0, BSD-3-Clause, Bundled or system native components, CC0-1.0 (public domain), Development only (never distributed) (+4 more)

### Community 143 - "ARCHITECTURE — Accord"
Cohesion: 0.17
Nodes (11): 1. Overview, 2. Layers and crates, 3. Local data model, 4. Identities and trust, 5. Groups: signed operation log, 6. Voice, 7. Structural decisions, 8. Threat model (summary — details in SECURITY.md) (+3 more)

### Community 144 - "run"
Cohesion: 0.27
Nodes (11): main(), parse_env(), Box, Error, ExitCode, Path, Result, String (+3 more)

### Community 145 - "friends.rs"
Cohesion: 0.31
Nodes (4): Reader<'a>, Result, String, Vec

### Community 146 - "LossEstimator"
Cohesion: 0.26
Nodes (10): detects_dropped_frames(), handles_seq_wraparound(), LossEstimator, no_loss_when_contiguous(), Self, VecDeque, seq_diff(), window_forgets_old_frames() (+2 more)

### Community 147 - "ErrorBoundary.tsx"
Cohesion: 0.20
Nodes (4): ErrorBoundary(), ErrorBoundaryInner, InnerProps, InnerState

### Community 148 - "ProfilePersonalizationDemo.tsx"
Cohesion: 0.20
Nodes (5): CompleteProfileCard(), root, ShowcaseTheme, AVATAR_DECORATIONS, PROFILE_EFFECTS

### Community 149 - "mnemonic.rs"
Cohesion: 0.27
Nodes (8): generate(), generate_and_restore_same_seed(), restore(), restore_tolerates_case_and_spacing(), Result, String, seed_from_entropy(), wrong_word_count_rejected()

### Community 150 - "seal"
Cohesion: 0.33
Nodes (10): derive(), group_key_sealed_size_is_80(), open(), Result, Vec, XNonce, seal(), seal_open_roundtrip() (+2 more)

### Community 151 - "Outbound"
Cohesion: 0.39
Nodes (5): CookiePacket, DataPacket, Packet, Vec, Welcome

### Community 152 - "boot_sim"
Cohesion: 0.25
Nodes (10): boot_sim(), eventually(), fast_maintenance(), invitation_complete_via_amorcage_par_defaut_deux_nat_symetriques(), resolve_eventually(), FnMut, Option, Path (+2 more)

### Community 153 - "voice_e2e.rs"
Cohesion: 0.24
Nodes (10): boot(), eventually(), F, Path, Vec, WsClient, tone(), two_nodes_join_voice_exchange_frames_and_respect_cap() (+2 more)

### Community 154 - "README.md"
Cohesion: 0.20
Nodes (7): Authenticity & disclaimer, Documentation, Features, Install, License, Quick start, Screenshots

### Community 155 - "scripts"
Cohesion: 0.20
Nodes (10): scripts, build, dev, format, format:check, lint, preview, tauri (+2 more)

### Community 156 - "audio.test.ts"
Cohesion: 0.12
Nodes (22): armAudioUnlock(), base64ToArrayBuffer(), ensureRunning(), extractBase64(), getAudioContext(), needsResume(), playClip(), playTones() (+14 more)

### Community 157 - "navPersistence.ts"
Cohesion: 0.33
Nodes (9): loadLastChannelByServer(), loadLastDm(), readStored(), removeStored(), saveLastChannelByServer(), saveLastDm(), STORAGE_KEYS, writeStored() (+1 more)

### Community 158 - "network_e2e.rs"
Cohesion: 0.36
Nodes (9): amorcage_resout_code_ami_puis_amitie_et_dm(), boot(), boot_port(), demarrer(), eventually(), port_p2p_stable_persiste_pour_reutilisation(), FnMut, Path (+1 more)

### Community 159 - "node_with_friend"
Cohesion: 0.22
Nodes (9): dm_edit_of_peer_message_is_refused(), dm_retry_rejects_delivered_message(), group_kick_ban_unban_and_leave(), group_roles_lifecycle_and_membership(), mentions_inbox_and_mark_read_roundtrip(), node_with_friend(), profile_of_friend_updates_friends_list(), Arc (+1 more)

### Community 160 - "service_on_disk_with_group"
Cohesion: 0.22
Nodes (9): group_emoji_add_del_and_state_shape(), group_emoji_add_validates_at_boundary(), group_send_sticker_via_groups_send_uses_registered_merkle_root(), group_set_banner_aller_retour_expose_dans_state(), group_set_member_avatar_set_and_clear(), group_stickers_add_del_list_and_state_shape(), group_stickers_add_validates_at_boundary(), TempDir (+1 more)

### Community 161 - "boot"
Cohesion: 0.36
Nodes (8): befriend(), boot(), eventually(), fast_maintenance(), group_sync_converge_apres_redemarrage(), presence_resolue_et_outbox_videe_apres_redemarrage(), FnMut, Path

### Community 162 - "tcp_link_e2e.rs"
Cohesion: 0.29
Nodes (7): enforce_origin(), host_without_port(), origin_allowed(), Option, HandshakeErrorResponse, HandshakeRequest, HandshakeResponse

### Community 163 - "files.test.ts"
Cohesion: 0.52
Nodes (4): BlockingSocket, Result, SocketAddr, Vec

### Community 164 - "service_with_voice"
Cohesion: 0.25
Nodes (8): NodeService, service_with_voice(), voice_devices_exact_shape_in_simulated_mode(), voice_join_status_mute_leave_exact_shapes(), voice_mic_test_is_explicitly_unavailable_without_hardware(), voice_params_are_validated_at_boundary(), voice_set_devices_persists_and_resets(), voice_set_devices_rejects_bad_params_at_boundary()

### Community 165 - "boot_sim"
Cohesion: 0.39
Nodes (7): boot_sim(), fast_maintenance(), resolve_eventually(), resout_le_code_ami_d_un_invite_nate(), Option, Path, SocketAddr

### Community 166 - "boot_sim"
Cohesion: 0.36
Nodes (7): boot_sim(), eventually(), fast_maintenance(), premier_contact_derriere_nat_symetrique_sans_port_ouvert(), FnMut, Path, SocketAddr

### Community 167 - "gain.rs"
Cohesion: 0.43
Nodes (6): apply_gain(), boost_saturates_instead_of_wrapping(), gain_of_pct(), half_gain_halves_samples(), unity_gain_leaves_samples_untouched(), zero_gain_silences()

### Community 168 - "Accord Threat Model — v0 accepted trade-offs"
Cohesion: 0.25
Nodes (8): 1. Trust architecture (recap), 2. Accepted trade-off: deterministic home relays, 3. Accepted trade-off: content-addressed blobs are bearer capabilities, 4. Accepted trade-off: server banner/icon is moderator-trusted content, 5. Accepted trade-off: DHT presence resolution exposes lookup metadata, 6. Accepted trade-off: legacy groups keep author-set op ids (grandfathered), 7. Out of scope for v0, Accord Threat Model — v0 accepted trade-offs

### Community 169 - "Accord local API — JSON-RPC 2.0 over WebSocket"
Cohesion: 0.29
Nodes (6): Accord local API — JSON-RPC 2.0 over WebSocket, Authentication, Error codes, Events (server → client notifications), Presence, Transport

### Community 170 - "automod.ts"
Cohesion: 0.62
Nodes (5): containsFiltered(), escapeRegExp(), maskFiltered(), usableWords(), wordPattern()

### Community 172 - "2. Cryptography"
Cohesion: 0.29
Nodes (7): 2.1 Identity, 2.2 Handshake (1-RTT, mutually authenticated), 2.3 Session state machine, 2.4 DATA packets and re-keying, 2.5 Handshake anti-DoS (COOKIE), 2.6 Storage at rest, 2. Cryptography

### Community 173 - "package.json"
Cohesion: 0.25
Nodes (7): description, engines, node, name, private, type, version

### Community 174 - "notificationSound.test.ts"
Cohesion: 0.33
Nodes (4): FakeAudioContext, FakeGain, FakeOscillator, WindowWithAudio

### Community 175 - "ringtone.test.ts"
Cohesion: 0.33
Nodes (4): FakeAudioContext, FakeGain, FakeOscillator, WindowWithAudio

### Community 177 - "String"
Cohesion: 0.33
Nodes (6): profile_shape(), Option, String, Value, Vec, sorted_keys()

### Community 178 - "4. Attackers considered"
Cohesion: 0.33
Nodes (6): 4.1 Passive network observer, 4.2 Active network attacker (interception, spoofing), 4.3 Malicious DHT node, 4.4 Malicious relay, 4.5 Disk theft (powered-off machine), 4. Attackers considered

### Community 180 - "6. CORE channel (0x02) — messaging, groups, presence"
Cohesion: 0.40
Nodes (5): 6.1 Direct messages, 6.2 Group op-log, 6.4 Group key and rotation, 6.5 User profile (D-027), 6. CORE channel (0x02) — messaging, groups, presence

### Community 181 - "bridge.test.ts"
Cohesion: 0.50
Nodes (3): chargerInterception(), hide, invoke

### Community 182 - "ci.sh"
Cohesion: 0.67
Nodes (3): PATH, ci.sh script, step()

### Community 195 - "@testing-library/jest-dom"
Cohesion: 0.21
Nodes (12): Ctx, CUSTOM_EMOJI_PX, HEADING_CLASS, lienSur(), MarkdownTextProps, renderNode(), renderNodes(), roleMentionStyle() (+4 more)

### Community 196 - "@testing-library/react"
Cohesion: 0.26
Nodes (5): FnMut, FnOnce, Option, Self, T

### Community 214 - ".decode"
Cohesion: 0.33
Nodes (4): RecordKind, Formatter, Result, Self

### Community 215 - "sound.ts"
Cohesion: 0.48
Nodes (5): estNomEmojiValide(), estMimeSonValide(), estNomSonValide(), estTailleSonValide(), SOUND_MIMES

### Community 217 - ".decode"
Cohesion: 0.43
Nodes (5): core_roundtrip(), forged_call_messages_are_rejected_without_panic(), invite_redeem_roundtrips_and_rejects_forged_bytes(), invite_ticket_expires_ms_bound_is_enforced_at_decode(), soundboard_play_roundtrips_and_rejects_forged_bytes()

### Community 224 - "NoopSender"
Cohesion: 0.20
Nodes (12): NoopSender, codec_factory(), materiel_codec_factory(), CodecFactory, FrameSender, Arc, Node, Send (+4 more)

## Knowledge Gaps
- **650 isolated node(s):** `name`, `private`, `version`, `description`, `type` (+645 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **20 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `NodeError` connect `NodeError` to `endpoint.rs`, `files.rs`, `helpers.rs`, `network.rs`, `Endpoint`, `Result`, `maintenance.rs`, `Node`, `Engine`, `search.rs`, `io.rs`, `now_ms`, `Node`, `VoiceHandle`, `engine.rs`, `EtatHote`, `Registry`, `lib.rs`, `Paths`, `SocketAddr`, `tests.rs`, `etat.rs`, `voice.rs`, `runtime.rs`, `Node`, `NoopSender`, `hex.rs`, `Node`?**
  _High betweenness centrality (0.124) - this node is a cross-community bridge._
- **Why does `CoreError` connect `Result` to `endpoint.rs`, `NodeError`, `Transfer`, `crypt.rs`, `profile.rs`, `DecodeError`, `CoreMsg`, `search.rs`, `files.rs`, `invite.rs`, `Identity`, `node_id_of`, `CoreError`, `mod.rs`, `Db`, `Result`, `mod.rs`, `presence.rs`, `offline.rs`, `IncomingInvite`, `Db`, `fec.rs`?**
  _High betweenness centrality (0.103) - this node is a cross-community bridge._
- **Why does `Identity` connect `identity.rs` to `Transfer`, `crypt.rs`, `Endpoint`, `NodeInfo`, `maintenance.rs`, `seal`, `Node`, `CoreMsg`, `node_with_friend`, `invite.rs`, `DhtRecord`, `Identity`, `relay_tunnel_e2e.rs`, `handshake.rs`, `node_id_of`, `mod.rs`, `types.rs`, `Paths`, `tests.rs`, `tests.rs`, `testnet.rs`, `offline.rs`?**
  _High betweenness centrality (0.041) - this node is a cross-community bridge._
- **What connects `name`, `private`, `version` to the rest of the system?**
  _650 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `api.ts` be split into smaller, more focused modules?**
  _Cohesion score 0.12 - nodes in this community are weakly interconnected._
- **Should `groups.ts` be split into smaller, more focused modules?**
  _Cohesion score 0.03533026113671275 - nodes in this community are weakly interconnected._
- **Should `useT` be split into smaller, more focused modules?**
  _Cohesion score 0.06155950752393981 - nodes in this community are weakly interconnected._