/**
 * Carte d'aperçu d'un lien — rendue **uniquement** si l'utilisateur a activé le
 * réglage (Paramètres → Confidentialité), éteint à l'installation.
 *
 * 🔒 Le composant est la seule porte : c'est ici que le réglage est consulté,
 * et la commande Tauri ne le revérifie pas (elle ne le connaît pas). Retirer la
 * garde ci-dessous ferait aller chercher chaque lien reçu sans que personne
 * l'ait demandé — voir `app/src-tauri/src/apercu_lien.rs` pour ce que ça coûte.
 */

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT, useUi } from '../stores/ui';

/** Ce que la commande `apercu_lien` rend. Champs facultatifs : page maigre. */
export interface Apercu {
  url: string;
  titre: string | null;
  description: string | null;
  image: string | null;
  hote: string;
}

/**
 * Première URL http(s) d'un texte, ou `null`.
 *
 * Une seule, volontairement : un message truffé de liens déclencherait autant
 * de requêtes, et c'est exactement le levier qu'utiliserait quelqu'un cherchant
 * à faire parler l'appareil d'en face.
 */
export function premierLien(texte: string): string | null {
  const m = /\bhttps?:\/\/[^\s<>"']+/i.exec(texte);
  if (m === null) return null;
  // La ponctuation finale appartient à la phrase, pas à l'URL.
  return m[0].replace(/[.,;:!?)\]]+$/, '');
}

export function LinkPreview({ texte }: { texte: string }) {
  const t = useT();
  const actif = useUi((s) => s.linkPreviews);
  const [apercu, setApercu] = useState<Apercu | null>(null);
  const url = premierLien(texte);

  useEffect(() => {
    // 🔒 La garde et l'appel dans le MÊME effet : séparer les deux laisserait
    // un chemin où l'un s'exécute sans l'autre.
    if (!actif || url === null) {
      setApercu(null);
      return;
    }
    let vivant = true;
    invoke<Apercu>('apercu_lien', { url })
      .then((a) => {
        if (vivant) setApercu(a);
      })
      // Échec silencieux : un aperçu manquant n'est pas une erreur pour
      // l'utilisateur, le lien reste cliquable au-dessus.
      .catch(() => {});
    return () => {
      vivant = false;
    };
  }, [actif, url]);

  if (apercu === null) return null;
  const titre = apercu.titre ?? apercu.hote;

  return (
    <a
      href={apercu.url}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={t.linkPreview.cardLabel}
      className="mt-1 block max-w-md rounded-md border-l-4 border-blurple bg-input/60 px-3 py-2 transition-colors duration-fast hover:bg-input"
    >
      <span className="block text-xs text-faint">{apercu.hote}</span>
      <span className="mt-0.5 block truncate text-sm font-medium text-norm">{titre}</span>
      {apercu.description !== null && (
        <span className="mt-0.5 block line-clamp-2 text-xs text-muted">
          {apercu.description}
        </span>
      )}
    </a>
  );
}
