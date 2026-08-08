# AGENTS.md

## Role

Tu travailles sur un projet de jeu en Rust.

Ton objectif n'est pas seulement de produire du code qui fonctionne localement, mais de l'integrer proprement dans l'architecture existante.

Ne genere pas du code isole qui repond uniquement a la demande immediate. Privilegie les modules de domaine coherents, les `struct`, `enum`, `trait`, blocs `impl`, et les responsabilites explicites.

## Ordre de lecture obligatoire

Avant toute session utile, lis les fichiers dans l'ordre defini par `ia_workflows/README.md`.

En l'absence d'indication plus precise, lis au minimum `ia_workflows/README.md` puis applique son ordre de lecture.

## Autorite des regles existantes

`ia_workflows/rules_globales.md` fait autorite pour :

- le mode de travail par plan ;
- la politique de compilation entre jalons ;
- le perimetre de modification ;
- les decisions a tracer ;
- la gestion des incertitudes ;
- la cloture de session ;
- la cloture de plan.

`ia_workflows/conventions_code.md` fait autorite pour :

- le decoupage par domaine ;
- le nommage des fichiers ;
- le nommage Rust du projet ;
- la structure du code ;
- le scope et la visibilite ;
- le style de formatage ;
- les conventions Bevy ;
- les traces et logs.

Ne remplace pas ces regles par des conventions Rust academiques, sauf demande explicite de l'utilisateur.

## Structure avant codage

Avant toute modification non triviale, identifie :

- le domaine qui possede le changement ;
- le fichier ou module concerne ;
- le type principal concerne ;
- le type qui doit posseder l'operation sous forme de methode ou de fonction associee ;
- si un nouveau type est necessaire ;
- quelles methodes doivent etre publiques ;
- quelles methodes doivent rester privees ;
- si une fonction libre est reellement justifiee ;
- si la modification risque de creer une architecture temporaire concurrente.

Si la demande conduit naturellement a ajouter plusieurs fonctions libres dispersees, propose d'abord une structure locale avant de coder.

## Politique sur les fonctions libres

Ne cree pas de fonctions libres par defaut.

Si une operation manipule principalement une structure existante, produit une donnee derivee depuis elle, ou porte son nom dans la signature, elle doit preferer une methode sur cette structure ou une fonction associee.

Exemple : une operation du type `build_low_preview_mesh_from_cache(cache: &LowSlabDerivedCache)` ne doit pas rester une fonction libre durable ; elle doit etre rattachee a `LowSlabDerivedCache` ou a un type metier dedie.

Une fonction libre est acceptable uniquement si :

- elle est purement utilitaire ;
- elle n'a pas de proprietaire metier naturel ;
- elle reste privee au module ;
- elle est explicitement justifiee dans le resume de fin de session.

Privilegie les methodes dans des blocs `impl`, les fonctions associees, ou des petits types metier dedies.

Un deplacement dans un sous-module ne suffit pas : si les fonctions restent orphelines, la refacto est incomplete et doit etre marquee comme temporaire dans le plan.

## Discipline de scope

Utilise toujours le scope Rust le plus petit possible.

Par defaut, un element est prive.

N'elargis la visibilite que si un consommateur reel l'exige.

Ordre de preference :

1. prive ;
2. `pub(super)` ;
3. `pub(crate)` ;
4. `pub`.

Ne rends pas un `struct`, une `enum`, un champ, une methode, une fonction ou un module public "au cas ou".

Si un type n'est utilise que par une seule structure, garde-le prive et proche de cette structure, idealement dans le meme fichier.

## Formatage

Respecte le style existant du projet.

Utilise les tabulations pour l'indentation.

Respecte le style inspire Allman du projet.

Ne lance pas de formatage global si cela reecrit du code non concerne ou remplace le style du projet par le style standard `rustfmt`.

Ne reformate pas les fichiers non concernes.

## Fin de session

A la fin d'une session utile, mets a jour les fichiers de suivi imposes par `ia_workflows/rules_globales.md`.

Resume ensuite :

- ce qui a change ;
- ou vit maintenant la logique principale ;
- les decisions prises ;
- les verifications executees ;
- ce qui n'a pas ete verifie ;
- le prochain jalon, la prochaine etape ou la prochaine validation attendue.
