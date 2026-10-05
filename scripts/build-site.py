"""Generates the project site (GitHub Pages) in site/: FR and EN landing pages,
sitemap.xml, robots.txt, llms.txt and llms-full.txt.

Run from the repository root:  python3 scripts/build-site.py
CI fails if the committed site differs from the generator output.
"""
import json, html, os, pathlib
ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
BASE = "https://phdhebde.github.io/Lectern/"
REPO = "https://github.com/Phdhebde/Lectern"
MODIFIED = "2026-10-05"
# GitHub Pages cannot send headers: the policy is set in a meta tag. The site runs no
# script at all (the JSON-LD block is data, not executed).
CSP = "default-src 'none'; style-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'"

C = {
"fr": dict(
  path="", lang="fr", locale="fr_FR", other="en", other_path="en/", other_label="English",
  title="Lectern — académie open source pour certifier partenaires et clients",
  desc="Plateforme open source en marque blanche pour former et certifier partenaires et clients : parcours, scénarios, examens, badges Open Badges, certificats PDF.",
  summary="Lectern est une plateforme open source (Rust, React, PostgreSQL) pour créer l'académie de votre logiciel : parcours, scénarios pratiques annotés, examens chronométrés, badges Open Badges 2.0 et certificats PDF, en marque blanche et auto-hébergée.",
  og="assets/og-fr.png",
  nav=[("#fonctionnalites","Fonctionnalités"),("#captures","Captures"),("#securite","Sécurité"),("#demarrage","Démarrage"),("#faq","FAQ")],
  h1="L'académie en marque blanche pour former et certifier vos partenaires et clients",
  lead="Lectern est une plateforme open source qui permet à un éditeur de logiciels de lancer sa propre académie : parcours de formation, scénarios pratiques illustrés, examens de certification et badges vérifiables, aux couleurs de sa marque et sur son infrastructure.",
  cta=[("#demarrage","Démarrer en 5 minutes","btn-primary"),(REPO,"Voir le code sur GitHub","btn-ghost"),(REPO+"/tree/main/docs","Documentation","btn-ghost")],
  tags=["Open source","Rust + React","PostgreSQL","OIDC / Keycloak","Open Badges 2.0","Kubernetes / K3s","RGPD"],
  facts_title="Lectern en bref",
  facts_intro="Les points clés, pour aller vite.",
  facts=[
    ("Ce que c'est","Une plateforme de formation et de certification (LMS de certification) auto-hébergée, en marque blanche."),
    ("Pour qui","Les éditeurs de logiciels qui forment et certifient leurs partenaires intégrateurs, leurs clients et le grand public."),
    ("Technologies","Serveur en Rust (axum, sqlx), interface React + TypeScript, base PostgreSQL, un seul binaire et une image Docker distroless."),
    ("Certifications","Badges SVG/PNG, certificats PDF, page publique de vérification, assertions Open Badges 2.0, ajout au profil LinkedIn."),
    ("Marque blanche","Nom, couleurs, polices, logo, textes, e-mails, badges et certificats viennent de la configuration : aucun nom de marque dans le code."),
    ("Déploiement","Docker, manifestes Kustomize pour Kubernetes/K3s et GitOps, sauvegardes quotidiennes, hébergement libre (France/UE possible)."),
  ],
  feat_title="Fonctionnalités", feat_intro="Tout ce qu'il faut pour un programme de certification partenaire, du premier module au palier Elite.",
  features=[
    ("Parcours et modules",["Vidéo (MP4 ou HLS) avec reprise de lecture","Fiche récapitulative, pièces jointes, quiz corrigé et expliqué","Prérequis et publics : public, partenaires, clients"]),
    ("Scénarios pratiques",["Pas à pas avec captures annotées (encadrés, flèches, numéros)","Mise en œuvre et diagnostic, résultat attendu, points d'attention","Éditeur visuel d'annotations à la souris"]),
    ("Examens de certification",["Tirage aléatoire dans une banque d'au moins 3× la taille de l'examen","Parties chronométrées côté serveur, études de cas inédites","Tentatives offertes, délai entre tentatives, correction manuelle avec grille"]),
    ("Badges et certificats",["Badge par parcours, certificat PDF nominatif","Page publique de vérification, Open Badges 2.0","Expiration, alertes à 90/30/7 jours, recertification"]),
    ("Organisations et paliers",["Rattachement par code, validé par le responsable formation","Tableau de bord : certifiés valides et écart avec chaque palier","Export CSV et API pour le portail partenaire"]),
    ("Administration",["Packs de contenu importables et exportables","Statistiques : complétion, réussite par question","Journal d'audit, rôles, tentatives supplémentaires"]),
  ],
  shots_title="Captures d'écran", shots_intro="Instance de démonstration avec un contenu fictif.",
  shots=[("scenario","Lecteur de scénario : capture annotée, action et résultat attendu"),("exam","Examen chronométré : questions tirées au hasard, réponses mélangées"),("certifications","Espace apprenant : certificat PDF, LinkedIn et page de vérification"),("organization","Tableau de bord du responsable formation et exigences par palier"),("verification","Page publique de vérification d'une certification"),("editor","Back-office : réglages d'un parcours et de son examen")],
  how_title="Comment ça marche",
  steps=[
    ("Inscription","connexion par SSO (OIDC) ou par lien e-mail, puis rattachement à une organisation validé par son responsable formation."),
    ("Parcours","modules courts avec vidéo, fiche et quiz ; la progression est enregistrée."),
    ("Scénarios","mise en œuvre et diagnostic pas à pas, à reproduire au besoin sur un environnement existant."),
    ("Examen","chronométré, tiré d'une banque de questions, avec seuil de réussite et délai entre tentatives."),
    ("Certification","badge, certificat PDF, page de vérification ; les certifiés valides comptent pour le palier de l'organisation."),
  ],
  sec_title="Sécurité", sec_intro="Conçue pour un éditeur de sécurité : la plateforme applique ce qu'elle enseigne.",
  sec=[
    "MFA obligatoire pour les rôles formateur et administrateur (via OIDC)",
    "Sessions en cookie HttpOnly, Secure, préfixe __Host-, protection CSRF et vérification d'origine",
    "CSP stricte sans script ni style en ligne, HSTS, en-têtes de sécurité",
    "Cloisonnement strict des données entre organisations",
    "Banques de questions et corrigés jamais envoyés aux apprenants",
    "Chaîne d'approvisionnement : cargo-deny, npm audit, CodeQL, Trivy, SBOM, actions épinglées",
    "Image d'exécution distroless, non-root, système de fichiers en lecture seule",
    "RGPD : export et suppression du compte en libre-service, page publique désactivable",
  ],
  cmp_title="Lectern ou un LMS généraliste ?",
  cmp_head=("Besoin","Lectern","LMS généraliste"),
  cmp=[
    ("Certifier des partenaires par palier","Exigences par palier et écart calculés","À construire soi-même"),
    ("Scénarios avec captures annotées","Lecteur et éditeur intégrés","Rarement disponible"),
    ("Intégrité des examens","Banque ≥ 3×, chronomètre serveur, cas inédits","Variable"),
    ("Badges vérifiables","Open Badges 2.0, page publique, LinkedIn","Souvent via un service tiers"),
    ("Marque blanche et données","Auto-hébergé, une instance = une marque","Souvent SaaS mutualisé"),
  ],
  start_title="Démarrage rapide", start_intro="Prérequis : Rust stable, Node.js 22 et PostgreSQL 16 (ou Docker Compose).",
  faq_title="Questions fréquentes",
  faq=[
    ("Qu'est-ce que Lectern ?","Lectern est une plateforme open source de formation et de certification en marque blanche. Elle permet à un éditeur de logiciels de proposer des parcours, des scénarios pratiques, des examens et des badges vérifiables à ses partenaires et clients."),
    ("Lectern est-il gratuit ?","Oui, le code est open source et publié sur GitHub. Vous l'hébergez vous-même ; les seuls coûts sont ceux de votre infrastructure."),
    ("Avec quelles technologies Lectern est-il construit ?","Le serveur est écrit en Rust (axum, sqlx) avec PostgreSQL ; l'interface est en React et TypeScript. L'ensemble tient dans un seul binaire et une image Docker distroless."),
    ("Les badges sont-ils compatibles Open Badges et LinkedIn ?","Oui. Chaque certification produit une assertion Open Badges 2.0 hébergée, une page publique de vérification, un badge PNG/SVG et un lien « Ajouter au profil » LinkedIn."),
    ("Comment personnaliser la marque ?","Un fichier de configuration TOML définit le nom, le logo, les couleurs (tokens CSS), les polices et les liens. Les textes, e-mails, badges et certificats se surchargent par fichiers, sans toucher au code."),
    ("Comment se connecter avec Keycloak ou un autre SSO ?","Lectern est un client OpenID Connect (code d'autorisation + PKCE). Il détecte la MFA via les claims acr/amr et peut synchroniser les rôles depuis un claim. La connexion par lien e-mail est aussi disponible."),
    ("Comment empêcher la triche aux examens ?","Questions tirées dans une banque d'au moins trois fois la taille de l'examen, réponses mélangées, parties chronométrées par le serveur, une seule session à la fois et études de cas différentes d'une tentative à l'autre."),
    ("Où héberger Lectern ?","Partout où tournent Docker et PostgreSQL. Des manifestes Kustomize sont fournis pour Kubernetes et K3s en GitOps, avec une sauvegarde quotidienne. L'hébergement en France ou dans l'UE est possible."),
    ("Lectern est-il conforme au RGPD ?","La plateforme collecte le minimum, permet l'export et la suppression du compte en libre-service et rend la page publique de vérification désactivable. La politique de confidentialité et les durées de conservation restent à définir par chaque opérateur."),
    ("Comment importer du contenu de formation ?","Les parcours, modules, scénarios et banques de questions sont décrits dans un pack de contenu (TOML, Markdown, images), importable et exportable depuis l'administration ou en ligne de commande."),
  ],
  footer_license="Licence", footer_docs="Documentation", footer_security="Sécurité", footer_updated="Mis à jour le",
  updated_human="5 octobre 2026",
  cmp_note="Comparaison indicative selon les besoins d'un programme de certification partenaire.",
),
"en": dict(
  path="en/", lang="en", locale="en_US", other="fr", other_path="", other_label="Français",
  title="Lectern — open-source academy to certify partners and customers",
  desc="Open-source, white-label platform to train and certify partners and customers: tracks, hands-on scenarios, exams, Open Badges and PDF certificates.",
  summary="Lectern is an open-source platform (Rust, React, PostgreSQL) to run your software company's academy: learning tracks, annotated hands-on scenarios, timed exams, Open Badges 2.0 badges and PDF certificates — white-label and self-hosted.",
  og="assets/og-en.png",
  nav=[("#features","Features"),("#screenshots","Screenshots"),("#security","Security"),("#quick-start","Quick start"),("#faq","FAQ")],
  h1="The white-label academy to train and certify your partners and customers",
  lead="Lectern is an open-source platform that lets a software vendor launch its own academy: learning tracks, illustrated hands-on scenarios, certification exams and verifiable badges, in its own brand and on its own infrastructure.",
  cta=[("#quick-start","Start in 5 minutes","btn-primary"),(REPO,"View the code on GitHub","btn-ghost"),(REPO+"/tree/main/docs","Documentation","btn-ghost")],
  tags=["Open source","Rust + React","PostgreSQL","OIDC / Keycloak","Open Badges 2.0","Kubernetes / K3s","GDPR"],
  facts_title="Lectern at a glance",
  facts_intro="The key points, fast.",
  facts=[
    ("What it is","A self-hosted, white-label training and certification platform (certification LMS)."),
    ("Who it is for","Software vendors training and certifying their integration partners, customers and the public."),
    ("Technology","Rust server (axum, sqlx), React + TypeScript interface, PostgreSQL, a single binary and a distroless Docker image."),
    ("Credentials","SVG/PNG badges, PDF certificates, public verification page, Open Badges 2.0 assertions, add-to-LinkedIn."),
    ("White label","Name, colours, fonts, logo, texts, e-mails, badges and certificates come from configuration — no brand in the code."),
    ("Deployment","Docker, Kustomize manifests for Kubernetes/K3s and GitOps, daily backups, host it anywhere (EU hosting possible)."),
  ],
  feat_title="Features", feat_intro="Everything a partner certification programme needs, from the first module to the top tier.",
  features=[
    ("Tracks and modules",["Video (MP4 or HLS) with resume","Recap sheet, attachments, quiz with corrections and explanations","Prerequisites and audiences: public, partners, customers"]),
    ("Hands-on scenarios",["Step by step with annotated screenshots (boxes, arrows, numbers)","Implementation and troubleshooting, expected results, pitfalls","Visual annotation editor"]),
    ("Certification exams",["Random draw from a bank at least 3× the exam size","Server-timed sections, unseen case studies","Free attempts, cooldowns, manual grading with a rubric"]),
    ("Badges and certificates",["One badge per track, named PDF certificate","Public verification page, Open Badges 2.0","Expiry, 90/30/7-day alerts, recertification"]),
    ("Organizations and tiers",["Join code approved by the training manager","Dashboard: valid certified people and gap per tier","CSV export and API for the partner portal"]),
    ("Administration",["Importable and exportable content packs","Statistics: completion, pass rate per question","Audit log, roles, extra attempts"]),
  ],
  shots_title="Screenshots", shots_intro="Demo instance with fictitious content.",
  shots=[("scenario","Scenario reader: annotated screenshot, action and expected result"),("exam","Timed exam: randomly drawn questions, shuffled answers"),("certifications","Learner space: PDF certificate, LinkedIn and verification page"),("organization","Training manager dashboard and tier requirements"),("verification","Public verification page of a certification"),("editor","Back office: track and exam settings")],
  how_title="How it works",
  steps=[
    ("Sign up","sign in with SSO (OIDC) or an e-mail link, then join an organization, approved by its training manager."),
    ("Learn","short modules with video, recap sheet and quiz; progress is saved."),
    ("Practise","step-by-step implementation and troubleshooting scenarios, to reproduce on an existing environment."),
    ("Exam","timed, drawn from a question bank, with a pass mark and cooldowns."),
    ("Certify","badge, PDF certificate, verification page; valid certifications count toward the organization's tier."),
  ],
  sec_title="Security", sec_intro="Built for a security vendor: the platform applies what it teaches.",
  sec=[
    "MFA required for trainer and administrator roles (via OIDC)",
    "HttpOnly, Secure, __Host- prefixed session cookies, CSRF protection and origin checks",
    "Strict CSP with no inline script or style, HSTS, security headers",
    "Strict data isolation between organizations",
    "Question banks and answer keys never sent to learners",
    "Supply chain: cargo-deny, npm audit, CodeQL, Trivy, SBOM, pinned actions",
    "Distroless, non-root runtime image with a read-only filesystem",
    "GDPR: self-service export and account deletion, public page can be hidden",
  ],
  cmp_title="Lectern or a general-purpose LMS?",
  cmp_head=("Need","Lectern","General-purpose LMS"),
  cmp=[
    ("Certify partners by tier","Tier requirements and gaps computed","Build it yourself"),
    ("Scenarios with annotated screenshots","Built-in reader and editor","Rarely available"),
    ("Exam integrity","Bank ≥ 3×, server timer, unseen cases","Varies"),
    ("Verifiable badges","Open Badges 2.0, public page, LinkedIn","Often a third-party service"),
    ("White label and data","Self-hosted, one instance = one brand","Often shared SaaS"),
  ],
  start_title="Quick start", start_intro="Requirements: stable Rust, Node.js 22 and PostgreSQL 16 (or Docker Compose).",
  faq_title="Frequently asked questions",
  faq=[
    ("What is Lectern?","Lectern is an open-source, white-label training and certification platform. It lets a software vendor offer learning tracks, hands-on scenarios, exams and verifiable badges to its partners and customers."),
    ("Is Lectern free?","Yes, the code is open source and published on GitHub. You host it yourself; the only costs are those of your infrastructure."),
    ("What is Lectern built with?","The server is written in Rust (axum, sqlx) with PostgreSQL; the interface uses React and TypeScript. It ships as a single binary and a distroless Docker image."),
    ("Are badges compatible with Open Badges and LinkedIn?","Yes. Each certification produces a hosted Open Badges 2.0 assertion, a public verification page, a PNG/SVG badge and a LinkedIn \"Add to profile\" link."),
    ("How do I apply my brand?","A TOML configuration file sets the name, logo, colours (CSS tokens), fonts and links. Texts, e-mails, badges and certificates are overridden with files, without touching the code."),
    ("How do I sign in with Keycloak or another SSO?","Lectern is an OpenID Connect client (authorization code + PKCE). It detects MFA from the acr/amr claims and can sync roles from a claim. E-mail link sign-in is also available."),
    ("How does Lectern prevent exam cheating?","Questions are drawn from a bank at least three times the exam size, answers are shuffled, sections are timed by the server, only one session runs at a time and case studies differ between attempts."),
    ("Where can I host Lectern?","Anywhere Docker and PostgreSQL run. Kustomize manifests are provided for Kubernetes and K3s with GitOps, including a daily backup. EU hosting is possible."),
    ("Is Lectern GDPR compliant?","The platform collects the minimum, offers self-service data export and account deletion, and lets learners hide their public verification page. Each operator still defines its privacy policy and retention periods."),
    ("How do I import training content?","Tracks, modules, scenarios and question banks are described in a content pack (TOML, Markdown, images) that can be imported and exported from the administration or the command line."),
  ],
  footer_license="License", footer_docs="Documentation", footer_security="Security", footer_updated="Updated on",
  updated_human="October 5, 2026",
  cmp_note="Indicative comparison for the needs of a partner certification programme.",
),
}

QUICK = """git clone https://github.com/Phdhebde/Lectern.git && cd Lectern
docker compose up -d db
export DATABASE_URL=postgres://lectern:lectern@localhost/lectern

# Demo content and an administrator
cargo run -p lectern-server -- --config config/lectern.example.toml import-pack examples/demo-pack
cargo run -p lectern-server -- --config config/lectern.example.toml grant-role you@example.com admin

# Server on http://localhost:8080 (front-end: cd web &amp;&amp; npm install &amp;&amp; npm run dev)
LECTERN__AUTH__REQUIRE_MFA_FOR='[]' LECTERN__SERVER__BRANDING_DIR=examples/branding \\
  cargo run -p lectern-server -- --config config/lectern.example.toml"""

e = html.escape
def page(c):
    url = BASE + c["path"]
    prefix = "../" if c["path"] else ""
    jsonld = {
      "@context":"https://schema.org",
      "@graph":[
        {"@type":"WebSite","@id":BASE+"#website","url":BASE,"name":"Lectern","inLanguage":["fr","en"]},
        {"@type":"WebPage","@id":url+"#page","url":url,"name":c["title"],"description":c["desc"],"inLanguage":c["lang"],
         "isPartOf":{"@id":BASE+"#website"},"about":{"@id":BASE+"#software"},"dateModified":MODIFIED,
         "primaryImageOfPage":BASE+c["og"]},
        {"@type":"SoftwareApplication","@id":BASE+"#software","name":"Lectern","url":BASE,
         "applicationCategory":"EducationalApplication","applicationSubCategory":"Learning management and certification",
         "operatingSystem":"Linux, Docker, Kubernetes","description":c["summary"],"isAccessibleForFree":True,
         "offers":{"@type":"Offer","price":"0","priceCurrency":"EUR"},
         "featureList":[f[0] for f in c["features"]],
         "screenshot":[BASE+"assets/img/"+s[0]+".webp" for s in c["shots"]],
         "softwareHelp":REPO+"/tree/main/docs","sameAs":[REPO]},
        {"@type":"SoftwareSourceCode","@id":BASE+"#code","name":"Lectern","codeRepository":REPO,
         "programmingLanguage":["Rust","TypeScript","SQL"],"runtimePlatform":"PostgreSQL",
         "license":REPO+"/blob/main/LICENSE","targetProduct":{"@id":BASE+"#software"}},
        {"@type":"FAQPage","@id":url+"#faq","inLanguage":c["lang"],"mainEntity":[
          {"@type":"Question","name":q,"acceptedAnswer":{"@type":"Answer","text":a}} for q,a in c["faq"]]},
      ]}
    ids = {"fr":["fonctionnalites","captures","securite","demarrage","faq"],"en":["features","screenshots","security","quick-start","faq"]}[c["lang"]]
    out = f"""<!doctype html>
<html lang="{c['lang']}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="{CSP}">
<meta name="referrer" content="strict-origin-when-cross-origin">
<title>{e(c['title'])}</title>
<meta name="description" content="{e(c['desc'])}">
<meta name="robots" content="index, follow, max-image-preview:large, max-snippet:-1">
<link rel="canonical" href="{url}">
<link rel="alternate" hreflang="{c['lang']}" href="{url}">
<link rel="alternate" hreflang="{c['other']}" href="{BASE + c['other_path']}">
<link rel="alternate" hreflang="x-default" href="{BASE}">
<link rel="icon" href="{prefix}assets/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="{prefix}assets/site.css">
<link rel="sitemap" type="application/xml" href="{BASE}sitemap.xml">
<link rel="alternate" type="text/plain" title="llms.txt" href="{BASE}llms.txt">
<meta name="theme-color" content="#1f3fbf">
<meta property="og:type" content="website">
<meta property="og:site_name" content="Lectern">
<meta property="og:title" content="{e(c['title'])}">
<meta property="og:description" content="{e(c['desc'])}">
<meta property="og:url" content="{url}">
<meta property="og:image" content="{BASE + c['og']}">
<meta property="og:image:width" content="1200">
<meta property="og:image:height" content="630">
<meta property="og:image:alt" content="Lectern">
<meta property="og:locale" content="{c['locale']}">
<meta property="og:locale:alternate" content="{'en_US' if c['lang']=='fr' else 'fr_FR'}">
<meta name="twitter:card" content="summary_large_image">
<meta name="twitter:title" content="{e(c['title'])}">
<meta name="twitter:description" content="{e(c['desc'])}">
<meta name="twitter:image" content="{BASE + c['og']}">
<script type="application/ld+json">
{json.dumps(jsonld, ensure_ascii=False, indent=1)}
</script>
</head>
<body>
<a class="skip" href="#main">{'Aller au contenu' if c['lang']=='fr' else 'Skip to content'}</a>
<header class="top">
 <div class="wrap">
  <a class="logo" href="{prefix or './'}"><img src="{prefix}assets/favicon.svg" alt="" width="32" height="32">Lectern</a>
  <nav aria-label="{'Navigation principale' if c['lang']=='fr' else 'Main navigation'}"><ul>
"""
    for href,label in c["nav"]:
        out += f'   <li><a href="{href}">{e(label)}</a></li>\n'
    out += f'   <li><a href="{prefix}{c["other_path"] or ""}{"" if (prefix or c["other_path"]) else "./"}" hreflang="{c["other"]}" lang="{c["other"]}">{c["other_label"]}</a></li>\n'
    out += f"""  </ul></nav>
 </div>
</header>
<main id="main">
<section class="hero">
 <div class="wrap">
  <h1>{e(c['h1'])}</h1>
  <p class="lead">{e(c['lead'])}</p>
  <div class="cta">
"""
    for href,label,cls in c["cta"]:
        out += f'   <a class="btn {cls}" href="{href}">{e(label)}</a>\n'
    out += '  </div>\n  <ul class="badges">' + "".join(f"<li>{e(t)}</li>" for t in c["tags"]) + "</ul>\n </div>\n</section>\n"
    out += f'<section class="alt" aria-labelledby="summary">\n <div class="wrap">\n  <h2 id="summary">{e(c["facts_title"])}</h2>\n  <p class="intro">{e(c["facts_intro"])}</p>\n  <ul class="facts">\n'
    for k,v in c["facts"]:
        out += f"   <li><strong>{e(k)}</strong>{e(v)}</li>\n"
    out += "  </ul>\n </div>\n</section>\n"
    out += f'<section id="{ids[0]}">\n <div class="wrap">\n  <h2>{e(c["feat_title"])}</h2>\n  <p class="intro">{e(c["feat_intro"])}</p>\n  <div class="grid">\n'
    for t,items in c["features"]:
        out += f'   <article class="card"><h3>{e(t)}</h3><ul>' + "".join(f"<li>{e(i)}</li>" for i in items) + "</ul></article>\n"
    out += "  </div>\n </div>\n</section>\n"
    out += f'<section class="alt" id="{ids[1]}">\n <div class="wrap">\n  <h2>{e(c["shots_title"])}</h2>\n  <p class="intro">{e(c["shots_intro"])}</p>\n  <div class="shots">\n'
    for i,(img,cap) in enumerate(c["shots"]):
        out += f'   <figure><img src="{prefix}assets/img/{img}.webp" alt="{e(cap)}" width="1200" height="750" loading="{"eager" if i==0 else "lazy"}" decoding="async"><figcaption>{e(cap)}</figcaption></figure>\n'
    out += "  </div>\n </div>\n</section>\n"
    out += f'<section aria-labelledby="how">\n <div class="wrap">\n  <h2 id="how">{e(c["how_title"])}</h2>\n  <ol class="steps">\n'
    for t,d in c["steps"]:
        out += f"   <li><strong>{e(t)}</strong> — {e(d)}</li>\n"
    out += "  </ol>\n </div>\n</section>\n"
    out += f'<section class="alt" id="{ids[2]}">\n <div class="wrap">\n  <h2>{e(c["sec_title"])}</h2>\n  <p class="intro">{e(c["sec_intro"])}</p>\n  <ul class="facts">\n'
    for s in c["sec"]:
        out += f"   <li>{e(s)}</li>\n"
    out += f'  </ul>\n  <p><a href="{REPO}/blob/main/docs/security.md">{"Lire la documentation de sécurité" if c["lang"]=="fr" else "Read the security documentation"}</a></p>\n </div>\n</section>\n'
    h = c["cmp_head"]
    out += f'<section aria-labelledby="cmp">\n <div class="wrap">\n  <h2 id="cmp">{e(c["cmp_title"])}</h2>\n  <div class="table-scroll"><table>\n   <caption class="intro">{e(c["cmp_note"])}</caption>\n   <thead><tr><th scope="col">{e(h[0])}</th><th scope="col">{e(h[1])}</th><th scope="col">{e(h[2])}</th></tr></thead>\n   <tbody>\n'
    for a,b,d in c["cmp"]:
        out += f'    <tr><th scope="row">{e(a)}</th><td>{e(b)}</td><td>{e(d)}</td></tr>\n'
    out += "   </tbody>\n  </table></div>\n </div>\n</section>\n"
    out += f'<section class="alt" id="{ids[3]}">\n <div class="wrap">\n  <h2>{e(c["start_title"])}</h2>\n  <p class="intro">{e(c["start_intro"])}</p>\n  <pre><code>{QUICK}</code></pre>\n  <p><a href="{REPO}/blob/main/docs/configuration.md">{"Installation en production" if c["lang"]=="fr" else "Production installation"}</a> · <a href="{REPO}/blob/main/docs/content-pack.md">{"Format des packs de contenu" if c["lang"]=="fr" else "Content pack format"}</a> · <a href="{REPO}/blob/main/docs/customization.md">{"Personnalisation" if c["lang"]=="fr" else "Customization"}</a></p>\n </div>\n</section>\n'
    out += f'<section class="faq" id="{ids[4]}">\n <div class="wrap">\n  <h2>{e(c["faq_title"])}</h2>\n'
    for q,a in c["faq"]:
        out += f"  <details><summary>{e(q)}</summary><p>{e(a)}</p></details>\n"
    out += " </div>\n</section>\n</main>\n"
    out += f"""<footer>
 <div class="wrap">
  <p>Lectern · {e(c['footer_updated'])} <time datetime="{MODIFIED}">{e(c['updated_human'])}</time></p>
  <ul>
   <li><a href="{REPO}">GitHub</a></li>
   <li><a href="{REPO}/tree/main/docs">{e(c['footer_docs'])}</a></li>
   <li><a href="{REPO}/blob/main/SECURITY.md">{e(c['footer_security'])}</a></li>
   <li><a href="{REPO}/blob/main/LICENSE">{e(c['footer_license'])}</a></li>
  </ul>
 </div>
</footer>
</body>
</html>
"""
    return out

def write(rel, content):
    path = SITE / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")

write("index.html", page(C["fr"]))
write("en/index.html", page(C["en"]))

write("sitemap.xml", f"""<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9" xmlns:xhtml="http://www.w3.org/1999/xhtml" xmlns:image="http://www.google.com/schemas/sitemap-image/1.1">
""" + "".join(f"""  <url>
    <loc>{BASE}{c['path']}</loc>
    <lastmod>{MODIFIED}</lastmod>
    <xhtml:link rel="alternate" hreflang="fr" href="{BASE}"/>
    <xhtml:link rel="alternate" hreflang="en" href="{BASE}en/"/>
    <xhtml:link rel="alternate" hreflang="x-default" href="{BASE}"/>
""" + "".join(f"""    <image:image><image:loc>{BASE}assets/img/{img}.webp</image:loc></image:image>
""" for img, _ in c["shots"]) + """  </url>
""" for c in (C["fr"], C["en"])) + """</urlset>
""")

# Search engines and AI answer engines (generative engine optimization) are welcome:
# the site is public documentation of an open-source project.
write("robots.txt", f"""User-agent: *
Allow: /

# AI search and answer engines
User-agent: GPTBot
Allow: /
User-agent: OAI-SearchBot
Allow: /
User-agent: ChatGPT-User
Allow: /
User-agent: ClaudeBot
Allow: /
User-agent: Claude-SearchBot
Allow: /
User-agent: Claude-User
Allow: /
User-agent: PerplexityBot
Allow: /
User-agent: Perplexity-User
Allow: /
User-agent: Google-Extended
Allow: /
User-agent: Applebot-Extended
Allow: /
User-agent: Bingbot
Allow: /
User-agent: MistralAI-User
Allow: /

Sitemap: {BASE}sitemap.xml
""")

en = C["en"]
facts = "\n".join(f"- {k}: {v}" for k, v in en["facts"])
features = "\n".join(f"- {t}: " + "; ".join(i) for t, i in en["features"])
faq = "\n\n".join(f"### {q}\n{a}" for q, a in en["faq"])
DOCS = ["architecture", "configuration", "customization", "content-pack", "certification", "operations", "security", "privacy", "api"]
write("llms.txt", f"""# Lectern

> {en['summary']}

{facts}

## Features
{features}

## Documentation
- [README]({REPO}/blob/main/README.md): overview, quick start, tests
""" + "".join(f"- [{d}]({REPO}/blob/main/docs/{d}.md)\n" for d in DOCS) + f"""
## Pages
- [Home (French)]({BASE}): project presentation in French
- [Home (English)]({BASE}en/): project presentation in English
- [Full documentation for LLMs]({BASE}llms-full.txt): README and every documentation page in one file

## FAQ

{faq}
""")

full = [f"# Lectern — full documentation\n\nSource: {REPO}\nGenerated: {MODIFIED}\n"]
for name in ["README.md"] + [f"docs/{d}.md" for d in DOCS]:
    full.append(f"\n\n---\n\n<!-- {name} -->\n\n" + (ROOT / name).read_text(encoding="utf-8"))
write("llms-full.txt", "".join(full))
