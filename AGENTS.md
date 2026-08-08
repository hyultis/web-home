# AGENTS.md

## Projet

WebHome est une page d'accueil web personnalisable developpee en Rust.

Le projet utilise Leptos pour le rendu SSR et l'hydratation WASM, Axum pour le serveur HTTP, et des modules configurables pour afficher notamment des liens, des notes, des flux RSS, la meteo et des mails.

Le developpement de WebHome assume explicitement l'utilisation d'agents IA. Ce fichier est volontairement versionne pour fournir un point d'entree public aux agents et aux contributeurs.

## Objectif

Une modification doit s'integrer proprement dans l'architecture existante, pas seulement fonctionner localement.

Avant d'ajouter du code, identifie le domaine concerne, le proprietaire logique du comportement et la frontiere d'execution touchee : serveur SSR, navigateur WASM ou code partage.

Evite le code isole, les abstractions decoratives, les refactos hors perimetre et les architectures temporaires concurrentes.

## Workflow local prive

Si `ia_workflows/README.md` existe dans l'environnement local, lis-le avant toute session utile puis applique son ordre de lecture et ses regles. Ce dossier local contient le workflow detaille utilise par le mainteneur et n'est volontairement pas versionne dans le depot public.

Si ce dossier n'est pas disponible dans un clone public, ne tente pas d'en deviner le contenu. Appuie-toi sur ce fichier, sur la documentation versionnee et sur l'architecture reelle du projet.

## Cartographie publique

- `src/front/` : pages, composants, modules et utilitaires executes dans l'interface Leptos ;
- `src/api/` : fonctions serveur, contrats d'echange et implementations reservees au SSR ;
- `src/entry.rs` : shell HTML, routeur et racine de l'application ;
- `src/main.rs` : demarrage Axum, configuration, sessions et middlewares serveur ;
- `src/global_security.rs` : primitives de hash et de generation de sels partagees ;
- `static/` : styles, traductions et assets servis au navigateur ;
- `config/` et `dynamic/` : donnees d'execution locales, non destinees au code source public.

## Regles publiques de contribution

- Respecte le decoupage par domaine et les responsabilites des types existants.
- Utilise le scope Rust le plus petit compatible avec les consommateurs reels.
- Preserve la separation entre code serveur, code navigateur et code partage.
- Ne contourne pas le chiffrement cote client pour les donnees utilisateur persistantes.
- Ne journalise jamais de mot de passe, d'identifiant sensible, de contenu mail ou de donnee dechiffree.
- Respecte le style existant : tabulations, accolades inspirees du style Allman et nommage local du module touche.
- Ne lance pas de reformatage global et ne reformate pas les fichiers non concernes.
- Preserve les modifications locales preexistantes qui ne font pas partie de la demande.
- N'ajoute pas de crate, ne modifie pas le suivi Git et ne publie rien sans que la tache le justifie explicitement.

## Verification

Choisis les verifications selon la frontiere touchee : serveur SSR, bibliotheque partagee, cible WASM ou build Leptos complet. Une modification exclusivement documentaire n'impose pas de compilation applicative.

Indique toujours ce qui a ete verifie et ce qui ne l'a pas ete.

## Fin de session

Resume les changements, l'emplacement de la logique principale, les decisions prises, les verifications executees et la prochaine validation attendue.
