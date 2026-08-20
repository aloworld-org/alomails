// French (français) catalog. Typed as `Partial<Catalog>`: any key not
// present here falls back to English, so this can grow incrementally
// without ever showing a blank label. Conventions: vouvoiement,
// French spacing before « ; : ! ? » via non-breaking spaces where the
// UI renders prose, guillemets « » for quoted names.
import type { Catalog } from "./en";

export const fr: Partial<Catalog> = {
  moduleSites: "Sites web",
  sitesAddFirstSection: "Ajouter votre première section",
  sitesAddressAvailable: "Disponible",
  sitesAddressTaken: "Déjà utilisée",
  sitesAddressNotChecked:
    "Saisissez une adresse valide pour vérifier sa disponibilité",
  sitesNameRequired: "Nommez votre site pour continuer.",
  sitesAddressRequired: "Saisissez une adresse de site pour continuer.",
  driveEmpty: "Ce dossier est vide. Téléversez un fichier ou créez un dossier.",
  driveEmptyTitle: "Rien ici pour l’instant",
  driveEmptyReadOnly: "Cet espace ne contient encore aucun fichier.",
  driveEmptyTrashTitle: "La corbeille est vide",
  driveEmptyTrash: "Les éléments supprimés apparaîtront ici.",
  driveFolderEmpty: "Ce dossier est vide",
  driveUploadHere: "Téléverser ici",
  driveSort: "Trier",
  driveSortNameAsc: "Nom (A–Z)",
  driveSortNameDesc: "Nom (Z–A)",
  driveSortNewest: "Plus récents",
  driveSortOldest: "Plus anciens",
  driveSortLargest: "Plus volumineux",
  driveSortSmallest: "Plus petits",
  driveView: "Affichage",
  driveViewExtraLarge: "Très grandes icônes",
  driveViewLarge: "Grandes icônes",
  driveViewMedium: "Icônes moyennes",
  driveViewSmall: "Petites icônes",
  driveViewList: "Liste",
  driveViewDetails: "Détails",
  driveViewTiles: "Mosaïques",
  driveViewContent: "Contenu",
  driveViewNavigationPane: "Volet de navigation",
  driveViewCompact: "Affichage compact",
  driveViewExtensions: "Extensions de noms de fichiers",
  taskCreating: "Création…",
  taskFilesAttachTo: "Joindre à la tâche",
  taskFilesDropHint:
    "Déposez des images ou fichiers ici, ou utilisez Ajouter une pièce jointe.",
  taskFilesNeedTask:
    "Créez d’abord une tâche, puis joignez-y des images et des fichiers.",
  taskFilesUploadError: "Impossible de joindre ces fichiers. Réessayez.",
  taskChooseFromDrive: "Choisir dans Drive",
  taskChooseFromDriveHint:
    "Joignez des fichiers existants sans les téléverser à nouveau.",
  taskSearchDrive: "Rechercher dans ce dossier",
  taskDriveBack: "Revenir au dossier précédent",
  taskNoDriveFiles: "Aucun fichier dans ce dossier.",
  taskAttachSelected: "Joindre la sélection",
  taskFilesSelected: (count: number) =>
    count === 1 ? "1 fichier sélectionné" : `${count} fichiers sélectionnés`,
  taskCreateOnDate: (date: string) => `Créer une tâche pour le ${date}`,
  // brand
  appName: "alo",
  tagline: "L’espace de travail souverain et natif à l’IA pour l’Europe.",

  // modules
  moduleHome: "Accueil",
  moduleMail: "Courrier",
  moduleAgenda: "Agenda",
  moduleChat: "Messagerie",
  moduleMeet: "Réunions",
  moduleDrive: "Fichiers",
  moduleDocs: "Documents",

  // Search
  moduleSearch: "Rechercher",
  searchPlaceholder: "Rechercher fichiers, tâches et courriels…",
  searchHint: "Cherchez fichiers et tâches par nom, et courriels par contenu.",
  searchNoResults: "Aucun résultat.",
  aiAskAbout: (q: string): string => `Demander à l’IA : « ${q} »`,
  aiSources: "Sources",
  aiUnconfigured:
    "L’IA n’est pas encore configurée — un administrateur peut ajouter un modèle. Voici les correspondances :",
  aiUnreachable: "Impossible de joindre l’IA. Voici les correspondances :",
  searchKind: (kind: string) =>
    kind === "task"
      ? "Tâche"
      : kind === "message"
        ? "Courriel"
        : kind === "folder"
          ? "Dossier"
          : kind === "doc"
            ? "Document"
            : kind === "base"
              ? "Base"
              : "Fichier",

  // Home dashboard
  homeGreetingMorning: "Bonjour",
  homeGreetingAfternoon: "Bon après-midi",
  homeGreetingEvening: "Bonsoir",
  homeWelcome: "Bienvenue dans l’espace de travail alo",
  homeStatUnreadEmails: "Courriels non lus",
  homeStatEvents: "Événements à venir",
  homeStatMessages: "Messages non lus",
  homeStatFiles: "Documents",
  homeGoToMail: "Aller au Courrier",
  homeViewAgenda: "Voir l’agenda",
  homeOpenChat: "Ouvrir la messagerie",
  homeOpenDrive: "Ouvrir les fichiers",
  homeComingSoonShort: "Bientôt disponible",
  homeRecent: "Récents",
  homeStarred: "Favoris",
  homeUnread: "Non lus",
  homeViewAll: "Tout voir",
  homeNoRecent: "Rien pour l’instant.",
  homeQuickActions: "Actions rapides",
  homeCompose: "Rédiger",
  homeCreateEvent: "Créer un événement",
  homeStartChat: "Démarrer une conversation",
  homeUploadFile: "Téléverser un fichier",
  homeCreateDoc: "Créer un document",
  homeToday: "Aujourd’hui",
  homeAgendaComingSoon:
    "Votre agenda apparaîtra ici dès que le calendrier sera disponible.",
  homeAskTitle: "Demandez tout à alo",
  homeAskBody: "Votre assistant IA pour tout votre travail.",
  homeAskCta: "Demander à alo",
  homeAskPlaceholder: "Demandez-moi ce que vous voulez…",
  homeAskUnavailable: "alo est indisponible pour le moment. Réessayez bientôt.",
  homeMailClearTitle: "Vous êtes à jour !",
  homeCalendarClearTitle: "Aucun événement aujourd’hui",
  homeTasksClearTitle: "Tout est en ordre !",
  moduleAi: "Demander à l’IA",

  // shell
  newButton: "Nouveau",
  appLauncher: "Applications",
  appLauncherFavorites: "Vos favoris",
  appLauncherAll: "Toutes les applications",
  appLauncherEdit: "Modifier les favoris",
  appLauncherDone: "Terminé",
  appLauncherCancel: "Annuler",
  appLauncherDragHint: "Glissez-déposez vos six applications favorites",
  appLauncherAddFavorite: "Ajouter aux favoris",
  appLauncherRemoveFavorite: "Retirer des favoris",
  userMenu: "Compte",
  language: "Langue",
  signOut: "Se déconnecter",

  // contacts (address book)
  contactsTitle: "Contacts",
  contactsOpen: "Contacts",
  contactsSearchPlaceholder: "Rechercher des contacts…",
  contactsEmpty: "Aucun contact pour l’instant. Ajoutez le premier.",
  contactsSearchEmpty: "Aucun contact ne correspond à votre recherche.",
  contactsLoadError: "Impossible de charger vos contacts.",
  contactsNew: "Nouveau contact",
  contactEdit: "Modifier le contact",
  contactFirstName: "Prénom",
  contactLastName: "Nom",
  contactDisplayName: "Nom affiché",
  contactEmail: "E-mail",
  contactPhone: "Téléphone",
  contactOrganization: "Organisation",
  contactJobTitle: "Fonction",
  contactNotes: "Notes",
  contactAddEmail: "Ajouter un e-mail",
  contactAddPhone: "Ajouter un téléphone",
  contactRemoveFieldNamed: (value: string) => `Retirer ${value}`,
  contactKindLabel: (value: string) => `Type de ${value}`,
  contactKindWork: "Professionnel",
  contactKindHome: "Personnel",
  contactKindMobile: "Mobile",
  contactKindOther: "Autre",
  contactSave: "Enregistrer",
  contactCancel: "Annuler",
  contactDelete: "Supprimer",
  contactDeleteConfirm: (name: string) =>
    `Supprimer ${name} ? Cette action est irréversible.`,
  contactNeedsName: "Ajoutez un nom ou au moins un e-mail.",
  contactSaveError: "Impossible d’enregistrer ce contact.",
  contactDeleteError: "Impossible de supprimer ce contact.",
  contactNoEmail: "Aucun e-mail",
  contactsImport: "Importer",
  contactsExport: "Exporter",
  contactsImporting: "Importation…",
  contactsImported: (n: number, skipped: number) =>
    skipped > 0
      ? `${n} contact${n === 1 ? "" : "s"} importé${n === 1 ? "" : "s"} (${skipped} ignoré${skipped === 1 ? "" : "s"}).`
      : `${n} contact${n === 1 ? "" : "s"} importé${n === 1 ? "" : "s"}.`,
  contactsImportError:
    "Impossible d’importer ce fichier. S’agit-il d’un export .vcf ?",
  contactsExportError: "Impossible d’exporter vos contacts.",
  contactsExportEmpty: "Vous n’avez pas encore de contacts à exporter.",

  // import mail (IMAP wizard)
  importOpen: "Importer du courrier",
  importTitle: "Importer le courrier d’un autre compte",
  importIntro:
    "Récupérez votre courrier récent de Gmail, Outlook ou tout compte IMAP dans votre boîte de réception.",
  importProvider: "Où se trouve votre courrier ?",
  importProviderGmail: "Gmail",
  importProviderOutlook: "Outlook",
  importProviderOther: "Autre (IMAP)",
  importServer: "Serveur de messagerie",
  importPort: "Port",
  importEmail: "Adresse e-mail",
  importPassword: "Mot de passe",
  importAppPasswordHint:
    "Pour Gmail et Outlook, un mot de passe d’application est requis, pas votre mot de passe habituel.",
  importStart: "Démarrer l’import",
  importRunning: "Import de votre courrier — cela peut prendre une minute…",
  importDone: (imported: number, skipped: number) =>
    skipped > 0
      ? `${imported} message${imported === 1 ? "" : "s"} importé${imported === 1 ? "" : "s"} (${skipped} déjà présent${skipped === 1 ? "" : "s"}).`
      : `${imported} message${imported === 1 ? "" : "s"} importé${imported === 1 ? "" : "s"}.`,
  importNeedsFields:
    "Saisissez le serveur, votre e-mail et votre mot de passe.",
  importClose: "Fermer",
  signedInAs: "Connecté en tant que",
  comingSoonTitle: "Bientôt disponible",
  comingSoonBody:
    "Cette partie de votre espace de travail arrive bientôt. Le Courrier est déjà prêt.",

  // auth — brand panel
  brandHeadline: "Votre espace de travail.\nVos serveurs.\nVos règles.",
  brandSubtitle:
    "Courrier, calendrier, messagerie et fichiers — souverains, natifs à l’IA et hébergés en Europe.",
  brandEuBadge: "Hébergé sur votre infrastructure · UE",
  // auth — brand panel, produit courrier autonome (alomails)
  brandHeadlineMail: "Votre courrier.\nVotre vie privée.\nVos règles.",
  brandSubtitleMail:
    "Une messagerie privée et native à l’IA — souveraine et hébergée en Europe.",
  brandEuBadgeMail: "Messagerie souveraine · Hébergée en Europe",

  // auth — sign in
  signInHeading: "Connexion",
  signInSubtitle: "Bon retour. Saisissez vos identifiants pour continuer.",
  emailLabel: "Adresse e-mail",
  emailPlaceholder: "vous@votredomaine.com",
  emailPlaceholderMail: "vous@alomails.com",
  passwordLabel: "Mot de passe",
  showPassword: "Afficher le mot de passe",
  hidePassword: "Masquer le mot de passe",
  rememberMe: "Se souvenir de moi",
  forgotPassword: "Mot de passe oublié ?",
  forgotPasswordNote:
    "Pour réinitialiser votre mot de passe, contactez votre administrateur.",
  signInButton: "Se connecter",
  signingIn: "Connexion…",
  orDivider: "ou",
  signInWithSso: "Se connecter avec le SSO",
  ssoComingSoon: "L’authentification unique arrive bientôt.",

  // auth — two-factor
  twoFactorTitle: "Authentification à deux facteurs",
  twoFactorSubtitle:
    "Saisissez le code à 6 chiffres de votre application d’authentification",
  twoFactorRecoverySubtitle: "Saisissez l’un de vos codes de récupération",
  twoFactorCodeLabel: "Code d’authentification",
  recoveryCodeLabel: "Code de récupération",
  recoveryPlaceholder: "xxxx-xxxx",
  verify: "Vérifier",
  verifying: "Vérification…",
  useRecoveryCode: "Utiliser plutôt un code de récupération",
  useAuthenticator: "Utiliser plutôt votre application d’authentification",
  backToSignIn: "Retour à la connexion",

  // auth — errors
  errorBadCredentials:
    "Cette adresse e-mail ou ce mot de passe est incorrect. Veuillez réessayer.",
  errorSecondFactor: "Saisissez votre code d’authentification pour continuer.",
  errorBadOtp: "Ce code est incorrect. Veuillez réessayer.",
  errorRateLimited: "Trop de tentatives. Patientez un instant et réessayez.",
  errorGeneric:
    "Une erreur s’est produite lors de la connexion. Veuillez réessayer.",
  errorNetwork:
    "Impossible de joindre le serveur. Vérifiez votre connexion et réessayez.",
  signingOut: "Déconnexion…",

  // signup — comptes personnels (ADR 0018)
  signupHeading: "Créez votre adresse alo personnelle",
  signupSubtitle:
    "Une messagerie privée et souveraine — sans publicité ni pistage.",
  signupAddressLabel: "Choisissez votre adresse",
  signupPickPlaceholder: "votrenom",
  signupRecoveryLabel: "Votre e-mail actuel",
  signupRecoveryHint:
    "Nous y enverrons un code de vérification — il servira aussi d'adresse de récupération.",
  signupSendCode: "Envoyer le code de vérification",
  signupSending: "Envoi…",
  signupChecking: "Vérification…",
  signupAvailable: "Cette adresse est disponible",
  signupTaken: "Cette adresse est déjà prise",
  signupReserved: "Cette adresse est réservée",
  signupInvalid: "Utilisez 3 à 64 lettres, chiffres, points ou tirets",
  signupVerifyHeading: "Saisissez votre code",
  signupVerifySubtitle: (recovery: string) =>
    `Nous avons envoyé un code à 6 chiffres à ${recovery}. Il expire dans 10 minutes.`,
  signupCodeLabel: "Code de vérification",
  signupPasswordLabel: "Choisissez un mot de passe",
  signupPasswordHint: "Au moins 8 caractères.",
  signupCreate: "Créer le compte",
  signupCreating: "Création de votre compte…",
  signupResend: "Renvoyer le code",
  signupVerifyError: "Ce code est incorrect ou a expiré. Veuillez réessayer.",
  signupBeginError: "Nous n'avons pas pu envoyer le code. Veuillez réessayer.",
  signupDoneHeading: "Tout est prêt",
  signupDoneBody: (email: string) =>
    `${email} est prête. Connectez-vous avec votre nouvelle adresse et votre mot de passe.`,
  signupGoToLogin: "Aller à la connexion",
  signupUnavailable:
    "Les inscriptions personnelles ne sont pas ouvertes pour le moment.",
  signupHaveAccount: "Vous avez déjà un compte ?",
  signupBackToLogin: "Se connecter",
  signupCreateLink: "Créer un compte personnel",

  // auth — réinitialisation du mot de passe
  resetHeading: "Réinitialiser votre mot de passe",
  resetSubtitle:
    "Saisissez votre adresse alo — nous enverrons un code de réinitialisation à votre boîte de secours.",
  resetAddressLabel: "Votre adresse alo",
  resetSendCode: "Envoyer le code",
  resetSending: "Envoi…",
  resetVerifyHeading: "Saisissez le code",
  resetVerifySubtitle: (address: string) =>
    `Si un compte alo existe pour ${address}, un code de réinitialisation est en route vers sa boîte de secours. Saisissez-le ci-dessous avec un nouveau mot de passe.`,
  resetNewPasswordLabel: "Nouveau mot de passe",
  resetSubmit: "Définir le mot de passe",
  resetSubmitting: "Enregistrement…",
  resetDoneHeading: "Mot de passe mis à jour",
  resetDoneBody:
    "Vous pouvez maintenant vous connecter avec votre nouveau mot de passe.",
  resetRequestError:
    "Impossible de démarrer la réinitialisation. Veuillez réessayer.",
  resetVerifyError: "Cela n'a pas fonctionné — vérifiez le code et réessayez.",

  // agenda (calendrier)
  agendaNewEvent: "Nouvel événement",
  agendaCalendars: "Agendas",
  agendaCalendar: "Agenda",
  agendaNewCalendar: "Nouvel agenda",
  agendaNewCalendarPrompt: "Nom du nouvel agenda",
  agendaDeleteCalendar: "Supprimer l'agenda",
  agendaToday: "Aujourd'hui",
  agendaPrev: "Précédent",
  agendaToolbarLabel: "Agenda",
  agendaViewLabel: "Affichage",
  agendaNext: "Suivant",
  agendaMonth: "Mois",
  agendaWeek: "Semaine",
  agendaAllDay: "Toute la journée",
  agendaEventTitle: "Ajouter un titre",
  agendaEventStart: "Début",
  agendaEventEnd: "Fin",
  agendaEventLocation: "Lieu",
  rsvpFrom: "De",
  rsvpAccept: "Accepter",
  rsvpMaybe: "Peut-être",
  rsvpDecline: "Refuser",
  rsvpAccepted: "Vous avez accepté cette invitation.",
  rsvpDeclined: "Vous avez refusé cette invitation.",
  rsvpTentative: "Vous avez répondu Peut-être.",
  replyResponded: "a répondu",
  replyFrom: (who: string, verb: string) => `${who} ${verb}`,
  replyApplied: "Mis à jour sur votre événement.",
  rsvpError: "Impossible d'envoyer votre réponse — veuillez réessayer.",
  cancelledTitle: "Annulé :",
  cancelledRemoved: "Retiré de votre agenda.",
  cancelledAbsent: "Cet évènement n'était pas dans votre agenda.",
  agendaEventGuests: "Invités",
  agendaGuestsPlaceholder: "nom@exemple.com, autre@exemple.com",
  agendaGuestsHint:
    "Nous enverrons à chaque invité une invitation qu'il pourra accepter dans son propre agenda.",
  agendaEventDescription: "Notes",
  agendaSave: "Enregistrer",
  agendaSaveThis: "Cet évènement",
  agendaSaveAll: "Toute la série",
  agendaDelete: "Supprimer",
  agendaDeleteThis: "Cet évènement",
  agendaDeleteAll: "Toute la série",
  agendaCancel: "Annuler",
  agendaNewEventTitle: "Nouvel événement",
  agendaEditEventTitle: "Modifier l'événement",
  agendaEndBeforeStart: "L'événement se termine avant de commencer.",
  agendaSaveError: "Impossible d'enregistrer l'événement. Veuillez réessayer.",
  agendaRepeat: "Répéter",
  agendaRepeatNone: "Ne se répète pas",
  agendaRepeatDaily: "Tous les jours",
  agendaRepeatWeekly: "Toutes les semaines",
  agendaRepeatWeekdays: "En semaine (lun.–ven.)",
  agendaRepeatMonthly: "Tous les mois",
  agendaRepeatYearly: "Tous les ans",
  // tâches
  moduleTasks: "Tâches",
  taskProjects: "Projets",
  taskNewProject: "Nouveau projet",
  taskNewProjectPrompt: "Nom du nouveau projet",
  taskMyPlate: "Mes tâches",
  taskProposals: "Suggestions",
  taskBoard: "Tableau",
  taskList: "Liste",
  taskQuickAdd: "Ajouter une tâche…",
  taskAdd: "Ajouter",
  taskColTodo: "À faire",
  taskColInProgress: "En cours",
  taskColDone: "Terminé",
  taskDueToday: "Aujourd’hui",
  taskDueTomorrow: "Demain",
  taskDueYesterday: "Hier",
  taskPrioNone: "Aucune",
  taskPrioLow: "Basse",
  taskPrioMedium: "Moyenne",
  taskPrioHigh: "Haute",
  taskFromEmail: "Depuis un e-mail",
  taskFromEvent: "Depuis un événement",
  taskOpenEmail: "Ouvrir l’e-mail d’origine",
  createTask: "Créer une tâche",
  suggestTasks: "Proposer des tâches depuis cet e-mail",
  taskCreatedFromMail: "Tâche créée depuis cet e-mail.",
  taskSuggesting: "Lecture de l’e-mail pour en extraire les actions…",
  taskNoSuggestions: "Aucune action trouvée dans cet e-mail.",
  taskSuggested: (n: number) =>
    n === 1
      ? "1 suggestion ajoutée à votre boîte de tâches."
      : `${n} suggestions ajoutées à votre boîte de tâches.`,
  taskAiOff: "L’IA est désactivée, aucune suggestion n’a pu être faite.",
  taskClose: "Fermer",
  taskDelete: "Supprimer",
  taskAssignee: "Responsable",
  taskAssigneePlaceholder: "nom@exemple.com",
  taskDue: "Échéance",
  taskPriority: "Priorité",
  taskDescription: "Description",
  taskDescriptionPlaceholder: "Ajouter des détails…",
  taskSubtasks: "Sous-tâches",
  taskAddSubtask: "Ajouter une sous-tâche…",
  taskComments: "Commentaires",
  taskAddComment: "Écrire un commentaire…",
  taskActivity: "Activité",
  taskEmpty: "Aucune tâche. Ajoutez-en une ci-dessus.",
  taskPlateEmpty: "Rien à faire. Vous êtes à jour.",
  taskNoProposalsTitle: "Vous êtes à jour",
  taskNoProposals:
    "Les suggestions apparaissent ici quand alo détecte des actions dans un e-mail.",
  taskAiSuggested: "Suggéré par l’IA",
  taskAccept: "Accepter",
  taskReject: "Ignorer",
  taskActivityKind: (kind: string) =>
    (
      ({
        created: "a créé cette tâche",
        status_changed: "l’a déplacée",
        assigned: "a changé le responsable",
        due_changed: "a changé l’échéance",
        commented: "a commenté",
        accepted: "a accepté la suggestion",
        proposed: "a été suggérée par l’IA",
      }) as Record<string, string>
    )[kind] ?? kind,
  agendaReminder: "Rappel",
  agendaReminderNone: "Aucun rappel",
  agendaReminderAtStart: "À l’heure de l’événement",
  agendaReminder5: "5 minutes avant",
  agendaReminder10: "10 minutes avant",
  agendaReminder15: "15 minutes avant",
  agendaReminder30: "30 minutes avant",
  agendaReminder60: "1 heure avant",
  agendaReminder1Day: "1 jour avant",
  agendaRsvpAccepted: "Accepté",
  agendaRsvpDeclined: "Refusé",
  agendaRsvpTentative: "Peut-être",
  agendaRsvpPending: "Pas encore de réponse",
  agendaCheckAvailability: "Vérifier la disponibilité",
  agendaAvailChecking: "Vérification…",
  agendaAvailAllFree: "Tout le monde est libre à ce moment-là.",
  agendaAvailBusy: (names: string) => `Occupé(s) : ${names}`,
  agendaAvailNoGuests: "Ajoutez des invités pour vérifier leur disponibilité.",
  agendaAvailError: "Impossible de vérifier la disponibilité.",
  agendaClose: "Fermer",
  agendaReadOnly: "Vous avez un accès en lecture seule à cet agenda.",
  // Partage d'agenda
  agendaShare: "Partager l'agenda",
  agendaShareTitle: (name: string) => `Partager « ${name} »`,
  agendaShareWith: "Partager avec",
  agendaSharePerson: "Une personne",
  agendaShareGroupOption: "Un groupe",
  agendaShareEmail: "Adresse e-mail",
  agendaShareEmailPlaceholder: "nom@exemple.com",
  agendaShareGroupPick: "Choisir un groupe…",
  agendaShareAccess: "Accès",
  agendaShareViewer: "Lecture",
  agendaShareEditor: "Modification",
  agendaShareGroup: "Groupe",
  agendaShareAdd: "Partager",
  agendaShareRemove: "Retirer",
  agendaShareRemoveFor: (name: string) => `Ne plus partager avec ${name}`,
  agendaShareEmpty: "Pas encore partagé.",
  agendaShareLoadError: "Impossible de charger les partages.",
  agendaShareError: "Impossible de mettre à jour le partage. Réessayez.",

  // mail
  mailLoading: "Chargement de votre courrier…",
  mailSearching: "Recherche…",
  mailFolders: "Dossiers",
  flaggedView: "Marqués",
  flagDueAdd: "Ajouter une échéance",
  flagDueToday: "Aujourd’hui",
  flagDueTomorrow: "Demain",
  flagDueNextWeek: "La semaine prochaine",
  flagDuePick: "Choisir une date…",
  flagDueClear: "Retirer l’échéance",
  flagDueLabel: (when: string) => `À faire ${when}`,
  flagDueOverdue: (when: string) => `En retard — échéance ${when}`,
  flagDueSet: "Définir une date de suivi",
  resizeFolders:
    "Redimensionner le panneau des dossiers (glisser, ou touches fléchées ; double-clic pour réinitialiser)",
  resizeMessages:
    "Redimensionner la liste des messages (glisser, ou touches fléchées ; double-clic pour réinitialiser)",
  collapseFolders: "Masquer les dossiers",
  expandFolders: "Afficher les dossiers",
  mailEmpty: "Aucun message ici pour l’instant.",
  mailSearchEmpty: "Aucun message ne correspond à votre recherche.",
  mailSelectPrompt: "Votre boîte de réception est prête",
  mailSelectBody:
    "Choisissez un message dans la liste pour ouvrir la conversation.",
  mailListError: "Impossible de charger les messages.",
  mailFolderError: "Impossible de charger vos dossiers.",
  mailRetry: "Réessayer",
  mailFrom: "De",
  mailTo: "À",
  mailNoSubject: "(sans objet)",
  mailUnknownSender: "Expéditeur inconnu",

  // mail — sidebar
  compose: "Écrire",
  mailSearchPlaceholder: "Rechercher dans le courrier…",
  viewAsMessages: "Afficher les messages individuellement",
  viewAsConversations: "Afficher par conversations",

  // mail — reading pane
  conversationActions: "Actions sur la conversation",
  reply: "Répondre",
  replyAll: "Répondre à tous",
  forward: "Transférer",
  archive: "Archiver",
  snooze: "Reporter",
  flag: "Marquer",
  unflag: "Retirer la marque",
  markRead: "Marquer comme lu",
  markUnread: "Marquer comme non lu",
  selectAll: "Tout sélectionner",
  selectNone: "Effacer la sélection",
  selectedCount: (n: number) =>
    n === 1 ? "1 sélectionné" : `${n} sélectionnés`,
  snoozeUntil: "Reporter jusqu’à…",
  snoozeLaterToday: "Plus tard aujourd’hui",
  snoozeTomorrow: "Demain",
  snoozeWeekend: "Ce week-end",
  snoozeNextWeek: "La semaine prochaine",
  mailSnoozed: "Reporté",
  delete: "Supprimer",
  dialogConfirm: "Confirmer",
  dialogCancel: "Annuler",
  dialogOk: "OK",
  deletePermanently: "Supprimer définitivement",
  moveTo: "Déplacer vers un dossier",
  moreActions: "Plus d’actions",
  mailMoved: "Message déplacé.",
  mailDeleted: "Message supprimé.",
  mailActionFailed: "Cela n’a pas fonctionné — veuillez réessayer.",
  endOfMessage: "Fin du message",
  threadMessages: "messages",
  aloSummary: "Résumé alo",
  summaryPending: "Résumé de la conversation en cours…",
  smartReplies: "Réponses suggérées",
  quickReplyHint: "Répondre à tous · Transférer ci-dessus",
  toLabel: "à",
  ccLabel: "cc",
  bccLabel: "cci",
  recipientsNone: "—",
  senderVerified: "Vérifié",
  senderVerifiedTitle:
    "Expéditeur authentifié — SPF, DKIM et DMARC tous validés",
  replyTo: "Répondre à",
  quickReplyTo: (name: string) => `Réponse rapide à ${name}`,
  replyToName: (name: string) => `Répondre à ${name}…`,
  draftWithAi: "Rédiger avec l’IA",
  attachments: "Pièces jointes",
  attach: "Joindre des fichiers",
  attachmentUploading: "Téléversement…",
  attachmentDownloading: "Téléchargement…",
  attachmentUploadFailed: "Impossible de téléverser ce fichier.",
  downloadAttachment: (name: string) => `Télécharger ${name}`,
  attachmentFailed: "Impossible de télécharger cette pièce jointe.",

  // mail — compose
  composeTitle: "Nouveau message",
  composeReplyTitle: "Répondre",
  composeForwardTitle: "Transférer",
  composeForwardPrefix: "Tr : ",
  composeForwardedIntro: "---------- Message transféré ----------",
  composeLabelFrom: "De :",
  composeLabelDate: "Date :",
  composeLabelSubject: "Objet :",
  composeLabelTo: "À :",
  composeReplyAllTitle: "Répondre à tous",
  composeFrom: "De",
  composeTo: "À",
  composeCc: "Cc",
  composeBcc: "Cci",
  composeSubject: "Objet",
  composeRecipientsPlaceholder: "nom@exemple.com, …",
  composeSubjectPlaceholder: "Objet",
  composeBodyPlaceholder: "Rédigez votre message…",
  composeSend: "Envoyer",
  composeSending: "Envoi…",
  composeDiscard: "Abandonner",
  composeCcToggle: "Cc",
  composeNoRecipients: "Ajoutez au moins un destinataire.",
  composeSendError: "Impossible d’envoyer votre message. Veuillez réessayer.",
  composeSent: "Message envoyé.",
  composeUndoWindow: "Envoi…",
  composeUndoSend: "Annuler",
  composeSendUndone: "Envoi annulé — votre message est dans les brouillons.",
  scheduleSend: "Envoi programmé",
  scheduleTomorrowMorning: "Demain matin",
  scheduleTomorrowAfternoon: "Demain après-midi",
  scheduleMondayMorning: "Lundi matin",
  schedulePickTime: "Choisir la date et l’heure",
  mailScheduled: (when: string) => `Envoi programmé pour ${when}.`,
  scheduleError: "Impossible de programmer votre message. Veuillez réessayer.",
  cancelSend: "Annuler l’envoi",
  sendCancelled:
    "Envoi programmé annulé — votre message est de retour dans les brouillons.",
  contactSuggestions: "Contacts correspondants",
  labelColor: "Couleur de l’étiquette",
  labelColorHint: "clic droit pour colorer",
  labelColorClear: "Aucune couleur",
  folderNew: "Nouveau dossier",
  folderNewSub: "Nouveau sous-dossier",
  folderRename: "Renommer",
  folderDelete: "Supprimer le dossier",
  folderNamePlaceholder: "Nom du dossier",
  folderDeleteConfirm: (name: string) =>
    `Supprimer le dossier « ${name} » ? Ses messages ne sont pas supprimés.`,
  folderActionFailed:
    "Cette modification de dossier n’a pas fonctionné — veuillez réessayer.",
  folderActions: (name: string) => `Options du dossier ${name}`,

  // Shared mailboxes / delegation
  sharedMailboxLabel: "Boîte aux lettres",
  sharedMailboxesHeading: "Boîtes partagées",
  sharedMyMailbox: "Ma boîte aux lettres",
  sharedReadOnly: "lecture seule",
  sharedNoSend:
    "Vous ne pouvez pas envoyer depuis cette boîte partagée — l’accès en envoi ne vous a pas été accordé.",
  settingsSharing: "Partage",
  settingsSharingHint:
    "Laissez vos collègues ouvrir et gérer votre boîte aux lettres. Accordez l’accès en envoi pour qu’ils puissent aussi envoyer en votre nom.",
  sharingNone: "Vous n’avez partagé votre boîte aux lettres avec personne.",
  sharingEmailPlaceholder: "Adresse d’un collègue",
  sharingAdd: "Partager",
  sharingAddError:
    "Partage impossible — vérifiez que l’adresse correspond à un collègue de votre organisation.",
  userShareAccess: "Accès partagé",
  delegateTitle: (email: string) => `Qui peut accéder à ${email}`,
  delegateIntro:
    "Les personnes ajoutées peuvent ouvrir et gérer cette boîte aux lettres. Autorisez l’envoi pour qu’elles puissent aussi envoyer depuis cette adresse.",
  delegatePeople: "Personnes ayant accès",
  delegateNone: "Personne d’autre n’y a accès pour l’instant.",
  delegateAdd: "Ajouter une personne",
  delegateReadOnly: "Lecture seule",
  delegateManage: "Peut gérer",
  delegateAccessLabel: "Niveau d’accès",
  delegateSendLabel: "Autorisation d’envoi",
  delegateSendNone: "Ne peut pas envoyer",
  delegateSendAs: "Envoyer en tant que",
  delegateSendOnBehalf: "Envoyer au nom de",
  delegateRemove: "Retirer l’accès",
  delegateRemoveFor: (email: string) => `Retirer l’accès de ${email}`,
  delegateFoldersFor: (email: string) =>
    `Limiter ${email} à certains dossiers`,
  delegateError:
    "Cette modification d’accès n’a pas fonctionné — veuillez réessayer.",
  delegateFoldersLabel: "Limiter à des dossiers",
  delegateWholeMailbox: "Toute la boîte aux lettres",
  delegateLimitFolders: "Limiter l’accès à des dossiers précis",
  delegateFoldersSave: "Enregistrer les dossiers",
  delegateFoldersCancel: "Annuler",

  // Categories
  categories: "Catégories",
  categorize: "Classer",
  categoryNew: "Nouvelle catégorie",
  categoryRename: "Renommer",
  categoryDelete: "Supprimer la catégorie",
  categoryNamePlaceholder: "Nom de la catégorie",
  categoryNoneHint:
    "Aucune catégorie pour l’instant — ajoutez-en une depuis la barre latérale.",
  categoryDeleteConfirm: (name: string) =>
    `Supprimer la catégorie « ${name} » ? Elle est retirée de tous les messages qui la portent.`,
  categoryActionFailed:
    "Cette modification de catégorie n’a pas fonctionné — veuillez réessayer.",
  categoryActions: (name: string) => `Options de la catégorie ${name}`,
  categoryClearFilter: "Afficher tous les messages",

  // alo Transfer
  transferLink: "lien",
  transferSharedFile: "📎 Fichier partagé",
  transferDownload: "Télécharger",
  transferExpires: (date: string) => `le lien expire ${date}`,
  transferExpiryTitle: "Durée de validité des liens de gros fichiers",
  transferExpiryOption: (days: number) =>
    days === 1 ? "1 jour" : `${days} jours`,
  blockSenderNamed: (email: string) => `Bloquer ${email}`,
  senderBlocked: (email: string) =>
    `${email} bloqué — ses messages vont désormais dans les indésirables.`,

  // Filters & rules
  settingsFilters: "Filtres et règles",
  settingsFiltersHint:
    "Les règles s’exécutent sur votre serveur à l’arrivée du courrier — même hors ligne. La première règle correspondante s’applique.",
  filtersLoadError: "Impossible de charger vos filtres.",
  filtersSaveError: "Impossible d’enregistrer vos filtres. Veuillez réessayer.",
  filterAddRule: "Ajouter une règle",
  filterNamePlaceholder: "Nom de la règle (facultatif)",
  filterWhen: "Quand un message arrive et que",
  filterDo: "Faire ceci",
  filterMatchAll: "tout correspond",
  filterMatchAny: "au moins un correspond",
  filterOr: "ou",
  filterFieldFrom: "De",
  filterFieldTo: "À",
  filterFieldCc: "Cc",
  filterFieldSubject: "Objet",
  filterOpContains: "contient",
  filterOpIs: "est exactement",
  filterValuePlaceholder: "valeur",
  filterAddCondition: "Ajouter une condition",
  filterRemoveCondition: "Retirer la condition",
  filterConditionField: (n: number) => `Condition ${n} : champ`,
  filterConditionOp: (n: number) => `Condition ${n} : correspondance`,
  filterConditionValue: (n: number) => `Condition ${n} : valeur`,
  filterRemoveConditionAt: (n: number) => `Retirer la condition ${n}`,
  filterRuleEnabled: (rule: string) => `Règle active : ${rule}`,
  filterFolderLabel: "Dossier de destination",
  filterActionFileInto: "Déplacer vers un dossier",
  filterActionMarkRead: "Marquer comme lu",
  filterActionStar: "Ajouter aux favoris",
  filterActionDelete: "Supprimer",
  filterSaveRule: "Enregistrer la règle",
  filterCancel: "Annuler",
  filterDelete: "Supprimer la règle",
  filterNeedsCondition: "Ajoutez au moins une condition avec une valeur.",
  filterNeedsAction: "Choisissez au moins une action.",
  composeWroteOn: "a écrit :",
  composeReplyPrefix: "Re : ",
  composeBack: "Retour",
  composeExpand: "Plein écran",
  composeCollapse: "Quitter le plein écran",
  composeMinimize: "Réduire",
  composeRestore: "Restaurer",
  showQuoted: "Afficher le texte cité",
  showOriginal: "Afficher l’original",
  downloadEml: "Télécharger le .eml",
  print: "Imprimer",
  reportSpam: "Signaler comme indésirable",
  notSpam: "Non indésirable",
  spamBannerTitle: "Ce message est dans les indésirables",
  spamReasonDmarc: (domain: string) =>
    `Nous n’avons pas pu confirmer qu’il provenait réellement de ${domain} — il a échoué à l’authentification DMARC, signe fréquent d’usurpation.`,
  spamReasonDkim:
    "Sa signature cryptographique (DKIM) n’a pas pu être validée, l’expéditeur n’a donc pas pu être vérifié.",
  spamReasonSpf: (domain: string) =>
    `Le serveur qui l’a envoyé n’est pas autorisé à envoyer du courrier pour ${domain} (échec SPF).`,
  spamReasonNone:
    "Nous n’avons détecté aucun problème de distribution avec ce message — il ressemble peut-être à du courrier que vous, ou une règle de filtrage, avez marqué comme indésirable auparavant.",
  spamBannerHint:
    "Si ce n’est pas un indésirable, remettez-le dans votre boîte de réception.",
  spamSenderFallback: "le domaine de l’expéditeur",
  unsubscribe: "Se désabonner",
  unsubscribeConfirm: (sender: string) =>
    `Se désabonner de ${sender} ? Nous demanderons à l’expéditeur de cesser de vous écrire.`,
  unsubscribed: "Désabonné — l’expéditeur a été prié de cesser.",
  unsubscribeFailed:
    "Désabonnement automatique impossible — essayez le lien dans le message.",
  unsubscribeOpened: "Page de désabonnement ouverte dans un nouvel onglet.",
  forwardAsAttachment: "Transférer en pièce jointe",
  blockSender: "Bloquer l’expéditeur",
  junkUnavailable: "Aucun dossier d’indésirables où déplacer ce message.",
  hideQuoted: "Masquer le texte cité",
  formatting: "Mise en forme du texte",
  bold: "Gras",
  italic: "Italique",
  underline: "Souligné",
  link: "Insérer un lien",
  linkPrompt: "URL du lien :",
  improve: "Améliorer",
  aiImproveFailed: "L’IA n’a pas pu réécrire cela pour le moment.",

  // account settings
  settingsOpen: "Paramètres",
  settingsTitle: "Paramètres du courrier",
  settingsTabGeneral: "Général",
  settingsTabOrg: "Organisation",
  settingsOooToggle: "Envoyer des réponses automatiques",
  settingsSignature: "Votre signature",
  settingsSignatureHint: "Ajoutée au bas des messages que vous envoyez…",
  settingsOrgFooter: "Pied de page de l’organisation",
  settingsOrgFooterHint:
    "Ajouté au courrier sortant de chaque utilisateur, après sa signature.",
  settingsOrgFooterPlaceholder:
    "ex. nom de l’entreprise, adresse, mentions légales…",
  settingsOutOfOffice: "Absence du bureau",
  settingsOutOfOfficeHint:
    "Répond automatiquement une fois à toute personne qui vous écrit pendant votre absence.",
  settingsOooSubjectPlaceholder: "Objet (facultatif) — ex. Absent du bureau",
  settingsOooMessagePlaceholder:
    "ex. Je suis absent jusqu’à lundi et vous répondrai à mon retour.",
  settingsOooNeedsMessage:
    "Ajoutez un message pour activer la réponse d’absence.",
  settingsSave: "Enregistrer",
  settingsSaved: "Enregistré.",
  settingsSaveError: "Impossible d’enregistrer vos paramètres.",
  settingsLoadError: "Impossible de charger vos paramètres.",

  // admin console
  adminTitle: "Administration",
  adminBackToalo: "Retour à alo",
  adminOpen: "Console d’administration",
  adminOverview: "Vue d’ensemble",
  adminOverviewIntro: "Votre organisation en un coup d’œil.",
  overviewUsers: "Utilisateurs",
  overviewStorage: "Stockage utilisé",
  overviewDeliverability: "Délivrabilité",
  overviewDeliverOk: "Tous les contrôles réussis",
  overviewDeliverAttention: "Attention requise",
  overviewAi: "IA",
  overviewOn: "Activée",
  overviewOff: "Désactivée",
  overviewManage: "Gérer",
  adminDomains: "Domaines",
  adminDomainsIntro:
    "Les domaines pour lesquels cette organisation envoie et reçoit du courrier, et leur vérification.",
  adminDomainsError: "Impossible de charger les domaines.",
  adminDomainsEmpty:
    "Aucun domaine pour l’instant. Ajoutez-en un pour le vérifier.",
  adminAddDomain: "Ajouter un domaine",
  dkimPublish:
    "Publiez cet enregistrement DKIM pour que votre courrier soit signé",
  dkimRotate: "Renouveler la clé DKIM",
  dkimRotateConfirm: (domain: string) =>
    `Renouveler la clé DKIM pour ${domain} ? Publiez le nouvel enregistrement ; conservez l’ancien jusqu’à ce que le courrier ne l’utilise plus.`,
  dkimRotated: (domain: string) =>
    `Nouvelle clé DKIM pour ${domain} — publiez l’enregistrement mis à jour.`,
  adminAudit: "Journal d’audit",
  adminAuditIntro: "Qui a modifié quoi, et quand. Les plus récents d’abord.",
  adminAuditError: "Impossible de charger le journal d’audit.",
  adminAuditEmpty: "Aucune action administrative enregistrée pour l’instant.",
  auditBy: (actor: string) => `par ${actor}`,
  auditUnknownActor: "système",
  auditUserCreate: "A créé un utilisateur",
  auditUserDelete: "A supprimé un utilisateur",
  auditUserAdmin: "A modifié les droits d’administration",
  auditAliasAdd: "A ajouté un alias",
  auditAliasRemove: "A retiré un alias",
  auditGroupCreate: "A créé un groupe",
  auditGroupDelete: "A supprimé un groupe",
  auditGroupAddress: "A modifié l’adresse d’une liste",
  auditDomainRegister: "A enregistré un domaine",
  auditDomainVerify: "A vérifié un domaine",
  auditDomainDelete: "A retiré un domaine",
  auditTenantCreate: "A créé l’organisation",
  auditTenantStatus: "A modifié le statut de l’organisation",
  auditTenantQuota: "A modifié le quota de stockage",

  // control plane
  controlOpen: "Plan de contrôle",
  controlTitle: "Plan de contrôle",
  controlDeniedTitle: "Accès opérateur requis",
  controlDeniedBody:
    "Le plan de contrôle est réservé aux opérateurs de la plateforme. Votre compte n’en est pas un — demandez à un opérateur si vous avez besoin d’un accès.",
  controlTenants: "Organisations",
  controlTenantsIntro: "Toutes les organisations de ce déploiement.",
  controlTenantsError: "Impossible de charger les organisations.",
  controlTenantsEmpty: "Aucune organisation pour l’instant. Créez la première.",
  controlDomains: "Domaines",
  controlDomainsIntro:
    "Les domaines pour lesquels chaque organisation peut envoyer et recevoir du courrier, et leur vérification.",
  controlDomainsError: "Impossible de charger les domaines.",
  controlDomainsEmpty: "Aucun domaine enregistré pour l’instant.",
  tenantAdd: "Nouvelle organisation",
  tenantName: "Nom de l’organisation",
  tenantNameHint: "Acme SARL",
  tenantAdminEmail: "E-mail du premier administrateur",
  tenantAdminPassword: "Mot de passe du premier administrateur",
  tenantAdminPasswordHint: "au moins 12 caractères",
  tenantCreate: "Créer l’organisation",
  tenantInvalid:
    "Un nom, une adresse d’administrateur valide et un mot de passe d’au moins 12 caractères sont requis.",
  tenantCreateError: "Impossible de créer cette organisation.",
  tenantActive: "Active",
  tenantSuspended: "Suspendue",
  tenantSuspend: "Suspendre",
  tenantResume: "Réactiver",
  tenantDelete: "Supprimer l’organisation",
  tenantDeleteConfirm: (name: string) =>
    `Supprimer définitivement « ${name} » et toutes ses données ? Cette action est irréversible.`,
  tenantUsage: (n: number, size: string) =>
    `${n === 1 ? "1 utilisateur" : `${n} utilisateurs`} · ${size}`,
  tenantQuota: "Quota",
  tenantQuotaPrompt: "Quota de stockage en Go (laisser vide pour illimité) :",
  tenantQuotaUnlimited: "illimité",
  tenantQuotaOf: (size: string) => `sur ${size}`,
  domainAdd: "Ajouter un domaine",
  domainTenant: "Organisation propriétaire",
  domainName: "Domaine",
  domainRegister: "Enregistrer",
  domainInvalid: "Choisissez une organisation et saisissez un domaine valide.",
  domainCreateError: "Impossible d’enregistrer ce domaine.",
  domainActionError: "Cela n’a pas fonctionné. Réessayez.",
  domainVerified: "Vérifié",
  domainUnverified: "Non vérifié",
  domainVerify: "Vérifier",
  domainDelete: "Retirer le domaine",
  domainOwnedBy: (tenant: string) => `Appartient à ${tenant}`,
  domainDeleteConfirm: (domain: string) =>
    `Retirer ${domain} de ce déploiement ?`,
  domainVerifiedOk: (domain: string) => `${domain} est vérifié.`,
  domainVerifyPending: (domain: string) =>
    `Aucun enregistrement DNS TXT correspondant trouvé pour ${domain} pour l’instant — publiez-le puis réessayez.`,
  domainPublishTitle: "Publiez cet enregistrement DNS",
  domainPublishIntro: (domain: string) =>
    `Pour prouver la propriété de ${domain}, publiez cet enregistrement TXT, puis cliquez sur Vérifier.`,
  domainRecordName: "Nom de l’enregistrement",
  domainRecordType: "Type",
  domainRecordValue: "Valeur",
  domainPublishDone: "Terminé",

  adminDeniedTitle: "Accès administrateur requis",
  adminDeniedBody:
    "Vous n’avez pas d’accès administrateur à cet espace de travail. Demandez à un administrateur de vous l’accorder si nécessaire.",
  adminSecurity: "Sécurité et confiance",
  adminSecurityIntro:
    "Comment votre domaine de courrier apparaît vu de l’extérieur. Ces contrôles interrogent le DNS en direct et la politique MTA-STS à chaque exécution.",
  securityFor: (domain: string) => `Contrôles pour ${domain}`,
  securityRecheck: "Relancer les contrôles",
  securityChecking: "Contrôles en direct en cours…",
  securityError: "Impossible d’exécuter les contrôles — veuillez réessayer.",
  securityPass: "Réussi",
  securityWarn: "Attention",
  securityFail: "Action requise",
  adminGroups: "Groupes et listes",
  adminGroupsIntro:
    "Groupes d’accès partagé et listes de diffusion qui distribuent le courrier à leurs membres.",
  adminNewGroup: "Nouveau groupe",
  adminGroupsError: "Impossible de charger les groupes.",
  groupName: "Nom du groupe",
  groupRename: "Renommer",
  groupCreate: "Créer le groupe",
  groupListBadge: "Liste",
  groupMembers: "Membres",
  groupMemberCount: (n: number) => (n === 1 ? "1 membre" : `${n} membres`),
  groupNoMembers: "Aucun membre pour l’instant.",
  groupListAddress: "Adresse de la liste",
  groupListAddressHint:
    "Le courrier envoyé à cette adresse est distribué à chaque membre. Laissez vide pour un simple groupe d’accès.",
  groupAddressSave: "Enregistrer l’adresse",
  groupAddressClear: "Désactiver la liste",
  groupAddMember: "Ajouter un membre",
  groupDelete: "Supprimer le groupe",
  groupDeleteConfirm: (name: string) =>
    `Supprimer le groupe « ${name} » ? Les membres conservent leur boîte aux lettres.`,
  groupCreateError:
    "Impossible de créer ce groupe — le nom est peut-être déjà pris.",
  groupAddressError:
    "Impossible de définir cette adresse — elle est peut-être déjà utilisée.",
  groupActionError: "Cela n’a pas fonctionné — veuillez réessayer.",
  groupClose: "Fermer",
  adminUsers: "Utilisateurs et boîtes aux lettres",
  adminUsersIntro:
    "Les personnes de votre organisation et leurs boîtes aux lettres.",
  adminAddUser: "Ajouter un utilisateur",
  adminUsersError: "Impossible de charger les utilisateurs.",
  userAdminBadge: "Administrateur",
  userManage: "Gérer",
  userUsage: (n: number, size: string) =>
    `${n === 1 ? "1 message" : `${n} messages`} · ${size}`,
  userEmail: "Adresse e-mail",
  userPassword: "Mot de passe",
  userNewPassword: "Nouveau mot de passe",
  userPasswordHint: "Au moins 8 caractères.",
  userCreate: "Créer l’utilisateur",
  userInvalid:
    "Saisissez une adresse valide et un mot de passe d’au moins 8 caractères.",
  userCreateError:
    "Impossible de créer cet utilisateur — l’adresse est peut-être déjà utilisée.",
  userReset: "Réinitialiser le mot de passe",
  userResetDone: "Mot de passe réinitialisé.",
  userAdminRole: "Administrateur de l’organisation",
  userAdminRoleFor: (email: string) =>
    `Accès administrateur de l’organisation pour ${email}`,
  userAdminHint:
    "Les administrateurs peuvent gérer les utilisateurs, les alias et les paramètres.",
  userAliases: "Alias",
  userAliasesHint:
    "Adresses supplémentaires qui aboutissent à cette boîte aux lettres.",
  userAliasPlaceholder: "alias@namel3ss.com",
  userAliasAdd: "Ajouter un alias",
  userDelete: "Supprimer l’utilisateur",
  userDeleteConfirm: (email: string) =>
    `Supprimer ${email} et tout son courrier ? Cette action est irréversible.`,
  userActionError: "Cela n’a pas fonctionné — veuillez réessayer.",
  userClose: "Fermer",
  adminAiProviders: "Fournisseurs d’IA",
  adminProviderEnabledFor: (name: string) => `${name} activé`,
  adminAiIntro:
    "Choisissez les modèles qui font fonctionner alo — auto-hébergés, ou vos propres clés d’API.",
  adminAddProvider: "Ajouter un fournisseur",
  adminManage: "Gérer",
  adminDefaultBadge: "Par défaut",
  adminMakeDefault: "Définir par défaut",
  adminProvidersError: "Impossible de charger les fournisseurs.",
  adminAiSelfHosted: "Auto-hébergé (recommandé)",
  adminAiSelfHostedHint:
    "Fonctionne sur votre propre infrastructure — aucune donnée ne quitte vos serveurs.",
  adminAiOwnKeys: "Vos propres clés d’API",
  adminAiOwnKeysHint:
    "Connectez un fournisseur externe avec votre clé. Les requêtes quittent votre serveur vers ce fournisseur.",
  adminAiFootnote:
    "Les fournisseurs auto-hébergés conservent toutes les données sur votre infrastructure. Les clés d’API externes envoient les requêtes et le contenu à ce fournisseur — choisissez selon votre politique de données.",
  providerConnected: "Connecté",
  providerKeyAdded: "Clé ajoutée",
  providerReady: "Prêt",
  providerNotConfigured: "Non configuré",
  kindOllama: "Ollama",
  kindalo: "alo IA",
  kindMistral: "Mistral (UE)",
  mistralDesc:
    "Modèles européens, hébergés dans l’UE. Ajoutez votre clé Mistral pour activer. Recommandé pour la souveraineté des données.",
  kindOpenai: "OpenAI",
  kindAnthropic: "Anthropic",
  kindCustom: "Point de terminaison personnalisé",
  builtInTag: "Intégré",
  ollamaDesc:
    "Modèles locaux sur votre serveur — Llama 3, Mistral, et plus. Entièrement privé.",
  aloDesc:
    "Modèle intégré, hébergé en UE et optimisé pour alo — pointez-le vers votre point de terminaison alo IA.",
  openaiDesc: "GPT-4o, GPT-4o mini. Ajoutez votre clé OpenAI pour activer.",
  anthropicDesc:
    "Modèles Claude. Ajoutez votre clé d’API Anthropic pour activer.",
  customDesc:
    "Toute API compatible OpenAI — vLLM auto-hébergé, Together, Groq, OpenRouter…",
  connectTitle: (name: string) => `Connecter ${name}`,
  configureTitle: (name: string) => `Configurer ${name}`,
  providerBaseUrl: "Point de terminaison de l’API",
  providerModel: "Modèle",
  providerModels: "Modèles activés",
  providerAddModel: "Ajouter",
  providerModelPlaceholder: "nom du modèle",
  providerRemoveModel: (name: string) => `Retirer ${name}`,
  providerApiKey: "Clé d’API",
  providerShowKey: "Afficher la clé",
  providerHideKey: "Masquer la clé",
  providerApiKeyKept:
    "Enregistré — laissez vide pour conserver la clé actuelle",
  providerApiKeyOptional: "Inutile pour un Ollama local",
  providerTest: "Tester la connexion",
  providerTestAgain: "Tester à nouveau",
  providerTesting: "Test en cours…",
  providerTestOk: (n: number) =>
    n === 1
      ? "Connexion vérifiée — 1 modèle accessible"
      : `Connexion vérifiée — ${n} modèles accessibles`,
  providerTestFail: "Impossible de joindre ce point de terminaison.",
  providerCancel: "Annuler",
  providerSave: "Enregistrer et activer",
  providerSaveError: "Impossible d’enregistrer ce fournisseur.",
  providerRequired: "Un point de terminaison et un modèle sont requis.",
  removeRecipient: (name: string) => `Retirer ${name}`,
  recipientCount: (n: number) =>
    n === 1 ? "1 destinataire" : `${n} destinataires`,

  aiComingSoon: "L’assistant IA arrive bientôt.",
  archiveUnavailable: "Aucun dossier d’archives où déplacer ce message.",

  // Docs
  docTitle: "Offre T3 — Proceq",
  docSaved:
    "Enregistré dans les fichiers · toutes les modifications sont enregistrées",
  docViewMode: "Affichage du document",
  docCanvasView: "Canevas",
  docCanvasViewHint: "Affichage flexible en canevas",
  docPageView: "Page",
  docPageViewHint: "Affichage de page pour l’impression",
  docFormattingToolbar: "Barre de mise en forme du document",
  docMenuFile: "Fichier",
  docMenuEdit: "Modifier",
  docMenuInsert: "Insérer",
  docMenuFormat: "Format",
  docPrint: "Imprimer",
  docInsertDivider: "Séparateur",
  docInsertPageBreak: "Saut de page",
  docZoom: "Zoom du document",
  docZoomOut: "Réduire le zoom",
  docZoomIn: "Augmenter le zoom",
  docParagraphStyle: "Style de paragraphe",
  docStyleParagraph: "Paragraphe",
  docStyleHeading1: "Titre 1",
  docStyleHeading2: "Titre 2",
  docStyleHeading3: "Titre 3",
  docStyleBulletList: "Liste à puces",
  docStyleNumberedList: "Liste numérotée",
  docStyleChecklist: "Liste de contrôle",
  docTextColor: "Couleur du texte",
  docHighlightColor: "Couleur de surbrillance",
  docHighlightNone: "Sans surbrillance",
  docColorDefault: "Couleur par défaut",
  docColorHex: "Hex",
  docColorOpacity: "Opacité",
  docColorEyedropper: "Choisir une couleur à l’écran",
  docBrandColors: "Couleurs de la marque",
  docSaveBrandColor: "Enregistrer la couleur de marque actuelle",
  docRemoveBrandColor: "Supprimer la couleur de marque",
  docColorRed: "Rouge",
  docColorOrange: "Orange",
  docColorYellow: "Jaune",
  docColorGreen: "Vert",
  docColorBlue: "Bleu",
  docColorPurple: "Violet",
  docIndent: "Augmenter le retrait",
  docOutdent: "Diminuer le retrait",
  docWords: "mots",
  docCharacters: "caractères",
  docInsertLink: "Insérer un lien",
  docLinkPrompt: "Saisissez l’adresse web du texte sélectionné",
  docInsertImage: "Insérer une image",
  docFindReplace: "Rechercher et remplacer",
  docFind: "Rechercher",
  docReplaceWith: "Remplacer par",
  docFindNext: "Suivant",
  docReplaceAll: "Tout remplacer",
  docPageSetup: "Mise en page",
  docPageSize: "Format de page",
  docPageLetter: "Lettre",
  docPageOrientation: "Orientation",
  docPagePortrait: "Portrait",
  docPageLandscape: "Paysage",
  docPageMargins: "Marges",
  docMarginsNormal: "Normales",
  docMarginsNarrow: "Étroites",
  docMarginsWide: "Larges",
  docHeader: "En-tête",
  docHeaderPlaceholder: "Texte d’en-tête",
  docFooter: "Pied de page",
  docFooterPlaceholder: "Texte de pied de page",
  docPageNumbers: "Afficher le numéro de page",
  docFontFamily: "Police",
  docFontSize: "Taille de police",
  docLineSpacing: "Interligne",
  docAddComment: "Ajouter un commentaire",
  docComment: "Commentaire",
  docCommentPlaceholder: "Écrire un commentaire…",
  docResolveComment: "Résoudre le commentaire",
  docReopenComment: "Rouvrir le commentaire",
  docSavePdf: "Enregistrer en PDF",
  docAiPlaceholder: "Dites à l’IA quoi écrire ou modifier…",
  docAiPropose: "Rédiger",
  docAiProposalLabel: "Proposition — à vérifier avant d’ajouter",
  docAiInsert: "Insérer",
  docAiDiscard: "Rejeter",
  docAiUnavailable: "L’IA n’est pas disponible pour le moment.",
  docAskAi: "Demander à l’IA",
  docEquation: "Équation",
  docEquationHint: "Formule mathématique (LaTeX)",
  docBlockGroupAdvanced: "Avancé",
  driveImporting: (name: string): string => `Importation de ${name}…`,
  driveImportNote:
    "Nous l’ouvrons en tant que alo Sheet. La mise en forme peut différer — votre fichier d’origine reste dans Drive, inchangé.",
  driveImportFailed: (name: string): string =>
    `Impossible d’importer ${name}. Vous pouvez toujours télécharger l’original.`,
  sheetDownloadXlsx: "Télécharger en Excel (.xlsx)",
  sheetDownloadXlsxShort: "Excel",
  sheetName: "Nom de la feuille",
  sheetSaved: "Enregistré",
  sheetExport: "Exporter",
  sheetMore: "Plus d’actions",
  sheetRibbon: "Mise en forme",
  sheetTabHome: "Accueil",
  sheetTabOthers: "Autres",
  sheetTabInsert: "Insertion",
  sheetTabDraw: "Dessin",
  sheetTabLayout: "Mise en page",
  sheetTabFormulas: "Formules",
  sheetTabData: "Données",
  sheetTabReview: "Révision",
  sheetTabView: "Affichage",
  sheetTabSoon: (name: string): string =>
    `Les outils ${name} arrivent bientôt.`,
  sheetGroupCellSize: "Taille des cellules",
  sheetRowHeight: "Hauteur de ligne",
  sheetColumnWidth: "Largeur de colonne",
  sheetAutoFitRow: "Ajuster la ligne",
  sheetAutoFitColumn: "Ajuster la colonne",
  sheetGroupVisibility: "Visibilité",
  sheetHideRow: "Masquer la ligne sélectionnée",
  sheetShowRows: "Afficher toutes les lignes",
  sheetHideColumn: "Masquer la colonne sélectionnée",
  sheetShowColumns: "Afficher toutes les colonnes",
  sheetGroupSheetOptions: "Options de feuille",
  sheetToggleGridlines: "Quadrillage",
  sheetGridlineColor: "Couleur du quadrillage",
  sheetGroupDirection: "Direction",
  sheetLeftToRight: "De gauche à droite",
  sheetRightToLeft: "De droite à gauche",
  sheetUndo: "Annuler",
  sheetRedo: "Rétablir",
  sheetGroupHistory: "Annuler",
  sheetGroupFont: "Police",
  sheetGroupBorders: "Bordures",
  sheetGroupRotation: "Rotation",
  sheetGroupAlignment: "Alignement",
  sheetGroupWrap: "Renvoi",
  sheetGroupMerge: "Fusionner",
  sheetWrapOverflow: "Débordement",
  sheetWrapText: "Renvoyer à la ligne",
  sheetWrapClip: "Tronquer",
  sheetMergeAll: "Fusionner tout",
  sheetMergeAcross: "Fusionner horizontalement",
  sheetMergeVertically: "Fusionner verticalement",
  sheetUnmerge: "Annuler la fusion",
  sheetGroupNumber: "Nombre",
  sheetFontFamily: "Police",
  sheetFontSize: "Taille de police",
  sheetBold: "Gras",
  sheetItalic: "Italique",
  sheetUnderline: "Souligné",
  sheetStrike: "Barré",
  sheetAlignLeft: "Aligner à gauche",
  sheetAlignCenter: "Centrer",
  sheetAlignRight: "Aligner à droite",
  sheetMerge: "Fusionner les cellules",
  sheetNumberFormat: "Format des nombres",
  sheetCellStyles: "Styles de cellule",
  sheetMoreStyles: "Plus de styles de cellule",
  sheetStyleDefault: "Standard",
  sheetStyleHeading1: "Titre 1",
  sheetStyleHeading2: "Titre 2",
  sheetStyleHeading3: "Titre 3",
  sheetStyleHeading4: "Titre 4",
  sheetStyleTitle: "Titre",
  sheetStyleSubtitle: "Sous-titre",
  sheetFormatGeneral: "Standard",
  sheetFormatNumber: "Nombre",
  sheetFormatCurrency: "Devise",
  sheetFormatPercentage: "Pourcentage",
  sheetFormatDate: "Date",
  sheetFormatText: "Texte",
  sheetFormatPreviewGeneral: "1234,56",
  sheetFormatPreviewNumber: "1 234,56",
  sheetFormatPreviewCurrency: "1 234,56 €",
  sheetFormatPreviewPercentage: "12,34 %",
  sheetFormatPreviewDate: "06/08/2026",
  sheetFormatPreviewText: "Texte",
  sheetFontGrow: "Agrandir la police",
  sheetFontShrink: "Réduire la police",
  sheetFontColor: "Couleur du texte",
  sheetFillColor: "Couleur de remplissage",
  sheetAlignTop: "Aligner en haut",
  sheetAlignMiddle: "Aligner au milieu",
  sheetAlignBottom: "Aligner en bas",
  sheetWrap: "Renvoyer à la ligne",
  sheetGroupCells: "Cellules",
  sheetInsert: "Insérer",
  sheetDelete: "Supprimer",
  sheetFormat: "Format",
  sheetMoreCellOptions: "Plus d’options de cellule",
  sheetSortFilter: "Trier et filtrer",
  sheetGroupClear: "Effacer",
  sheetGroupRows: "Lignes",
  sheetGroupColumns: "Colonnes",
  sheetGroupView: "Fenêtre",
  sheetInsertRowAbove: "Insérer une ligne au-dessus",
  sheetInsertRowBelow: "Insérer une ligne en dessous",
  sheetInsertColLeft: "Insérer une colonne à gauche",
  sheetInsertColRight: "Insérer une colonne à droite",
  sheetDeleteRow: "Supprimer la ligne",
  sheetDeleteColumn: "Supprimer la colonne",
  sheetClearContents: "Effacer le contenu",
  sheetClearFormats: "Effacer la mise en forme",
  sheetFreeze: "Figer les volets",
  sheetUnfreeze: "Libérer",
  sheetGroupClipboard: "Presse-papiers",
  sheetGroupStyles: "Styles",
  sheetGroupEditing: "Édition",
  sheetGroupSortFilter: "Trier et filtrer",
  sheetGroupDataTools: "Outils de données",
  sheetGroupCharts: "Graphiques",
  sheetChartBar: "Graphique en barres",
  sheetChartLine: "Graphique en courbes",
  sheetChartPie: "Graphique circulaire",
  sheetCharts: "Graphiques de cette feuille",
  sheetChartRemove: "Supprimer le graphique",
  sheetChartSelectionHint: "Sélectionnez une ligne d’en-tête, une colonne de catégories et au moins une série numérique.",
  sheetChartExcelLimit: "Les graphiques restent dynamiques dans alo Sheet. L’export Excel inclut actuellement les cellules, mais pas ces graphiques.",
  sheetChartSeries: (number: number) => `Série ${number}`,
  chartTabMissing: "L’onglet utilisé par ce graphique n’existe plus.",
  chartRangesRagged: "Les plages du graphique n’ont plus la même longueur.",
  chartTooLarge: "Cette sélection est trop grande pour être affichée en toute sécurité.",
  sheetGroupProtection: "Protection",
  sheetGroupFreeze: "Figer les volets",
  sheetGroupZoom: "Zoom",
  sheetGroupInsertObjects: "Objets",
  sheetGroupDrawing: "Dessin",
  sheetGroupNotes: "Notes",
  sheetGroupComments: "Commentaires",
  sheetGroupFunctionLibrary: "Bibliothèque de fonctions",
  sheetGroupMoreFunctions: "Plus de fonctions",
  sheetAutoSum: "Somme automatique",
  sheetAverage: "Moyenne",
  sheetCount: "Nombre",
  sheetMinimum: "Minimum",
  sheetMaximum: "Maximum",
  sheetMoreFunctions: "Parcourir les fonctions",
  sheetGroupFunctionCategories: "Catégories de fonctions",
  sheetFormulaFinancial: "Financier",
  sheetFormulaDateTime: "Date et heure",
  sheetFormulaMathTrig: "Maths et trigonométrie",
  sheetFormulaStatistical: "Statistiques",
  sheetFormulaLookup: "Recherche et référence",
  sheetFormulaDatabase: "Base de données",
  sheetFormulaText: "Texte",
  sheetFormulaLogical: "Logique",
  sheetFormulaInformation: "Information",
  sheetFormulaEngineering: "Ingénierie",
  sheetFormulaCube: "Cube",
  sheetFormulaCompatibility: "Compatibilité",
  sheetFormulaWeb: "Web",
  sheetFormulaArray: "Tableau",
  sheetDataValidation: "Validation des données",
  sheetConditionalFormatting: "Mise en forme conditionnelle",
  sheetTextToColumns: "Convertir en colonnes",
  sheetNamedRanges: "Plages nommées",
  sheetProtectRange: "Protéger la plage",
  sheetUnprotectRange: "Ôter la protection de la plage",
  sheetProtectSheet: "Protéger la feuille",
  sheetUnprotectSheet: "Ôter la protection de la feuille",
  sheetProtectedRangeName: "Plage protégée",
  sheetProtectedSheetName: "Feuille protégée",
  sheetFreezeTopRow: "Figer la première ligne",
  sheetFreezeFirstColumn: "Figer la première colonne",
  sheetZoomOut: "Zoom arrière",
  sheetZoomReset: "100 %",
  sheetZoomIn: "Zoom avant",
  sheetInsertTable: "Tableau",
  sheetInsertLink: "Lien",
  sheetInsertImage: "Image",
  sheetDrawingPanel: "Images et dessin",
  sheetNote: "Ajouter ou modifier une note",
  sheetAddComment: "Nouveau commentaire",
  sheetCommentsPanel: "Volet des commentaires",
  sheetPaste: "Coller",
  sheetCut: "Couper",
  sheetCopy: "Copier",
  sheetPercent: "Pourcentage",
  sheetCurrency: "Devise",
  sheetComma: "Séparateur de milliers",
  sheetSortAsc: "Trier A → Z",
  sheetSortDesc: "Trier Z → A",
  sheetFilter: "Activer le filtre",
  sheetFindReplace: "Rechercher et remplacer",
  sheetBorders: "Bordures",
  sheetBordersAll: "Toutes les bordures",
  sheetBordersOuter: "Bordure extérieure",
  sheetBordersInside: "Bordures intérieures",
  sheetBordersTop: "Bordure supérieure",
  sheetBordersBottom: "Bordure inférieure",
  sheetBordersLeft: "Bordure gauche",
  sheetBordersRight: "Bordure droite",
  sheetBordersHorizontal: "Bordures horizontales",
  sheetBordersVertical: "Bordures verticales",
  sheetBordersNone: "Aucune bordure",
  sheetBordersAdvanced: "Bordures diagonales",
  sheetBordersDiagonalDown: "Bordure diagonale descendante",
  sheetBordersDiagonalUp: "Bordure diagonale montante",
  sheetBordersDiagonalDownCenter: "Diagonale descendante avec lignes centrales",
  sheetBordersDiagonalDownBoth:
    "Diagonale descendante avec les deux lignes centrales",
  sheetBordersDiagonalUpCenter: "Diagonale montante avec lignes centrales",
  sheetRotation: "Rotation",
  sheetRotationNone: "Aucune rotation",
  sheetRotation45: "Pivoter de 45° dans le sens horaire",
  sheetRotationMinus45: "Pivoter de 45° dans le sens antihoraire",
  sheetRotation90: "Pivoter de 90° dans le sens horaire",
  sheetRotationMinus90: "Pivoter de 90° dans le sens antihoraire",
  sheetRotationVertical: "Texte vertical",
  docShare: "Partager",
  docInsert: "Insérer",
  insertEquation: "Équation",
  insertCrossRef: "Renvoi",
  tbNormalText: "Texte normal",
  tbEditing: "Édition",
  specTitle: "Transfert thermique dans le panneau Coateq",
  specSubtitle: "Spécification technique · Rév. 3",
  specLead1: "Le flux en régime permanent est régi par",
  specLead2: "à travers la frontière.",
  specMid:
    "où k est la conductivité thermique et r₁, r₂ sont les rayons intérieur et extérieur. En substituant les valeurs mesurées :",
  specBcHeading: "Conditions aux limites",
  specRefLead: "En combinant",
  specRefMid: "avec les valeurs de",
  specRefTail: "on obtient les nombres ci-dessous.",
  tblSymbol: "Symbole",
  tblValue: "Valeur",
  eqTitle: "Équation",
  eqClose: "Fermer",
  eqInsert: "Insérer",
  eqPlaceholder: "ex.  E = mc^2",
  eqInputLabel: "Source LaTeX",
  eqPreview: "Aperçu",
  eqEmpty: "Commencez à saisir du LaTeX ci-dessus.",
  eqError: (message: string) => `Impossible d’afficher ce LaTeX : ${message}`,
  eqNumbered: "Numérotée",
  eqEmptyBlock: "Équation vide — cliquez pour modifier",
  eqSearchLabel: "Rechercher des symboles",
  eqSearchPlaceholder: "Rechercher des symboles — ex. somme, alpha, flèche",
  eqSearchClear: "Effacer la recherche",
  eqNoMatches: "Aucun symbole ne correspond à votre recherche.",
  eqCatStructures: "Structures",
  eqCatStyles: "Polices et styles",
  eqCatGreek: "Grec",
  eqCatOperators: "Opérateurs",
  eqCatRelations: "Relations",
  eqCatSets: "Ensembles et logique",
  eqCatArrows: "Flèches",
  eqCatBigops: "Grands opérateurs",
  eqCatCalculus: "Analyse",
  eqCatDelimiters: "Délimiteurs",
  eqCatMisc: "Symboles",
  composeInsertEquation: "Insérer une équation",
  composeInsertCode: "Insérer un bloc de code",
  strikethrough: "Barré",
  textColor: "Couleur du texte",
  highlight: "Surligner",
  bulletList: "Liste à puces",
  numberedList: "Liste numérotée",
  alignLeft: "Aligner à gauche",
  alignCenter: "Centrer",
  alignRight: "Aligner à droite",
  horizontalRule: "Séparateur",
  insertImage: "Insérer une image",
  clearFormatting: "Effacer la mise en forme",
  textStyle: "Style de texte",
  styleQuote: "Citation",
  fontFamily: "Police",
  fontSize: "Taille de police",
  sizeSmall: "Petite",
  sizeNormal: "Normale",
  sizeLarge: "Grande",
  sizeHuge: "Très grande",
  codeInsertTitle: "Insérer un bloc de code",
  codeInsertHint: "⌘/Ctrl + Entrée pour insérer",
  codePreviewLabel: "Aperçu — rendu dans le courriel",
  insertCancel: "Annuler",
  insertConfirm: "Insérer",
  docsTitle: "alo Documents",
  docsNew: "Nouveau document",
  docsEmpty:
    "Aucun document pour l’instant. Créez-en un pour commencer à écrire.",
  docsDelete: (title: string) => `Supprimer ${title}`,
  docsAll: "Tous les documents",
  docsUntitled: "Document sans titre",
  docsTitleLabel: "Titre du document",
  docsSaving: "Enregistrement…",
  docsSaved: "Enregistré",
  docsSaveError: "Impossible d’enregistrer",
  blockAdd: "Ajouter un bloc",
  blockMoveUp: "Déplacer le bloc vers le haut",
  blockMoveDown: "Déplacer le bloc vers le bas",
  blockDelete: "Supprimer le bloc",
  blockEmptyHint:
    "Ajoutez un titre, du texte, une équation, du code ou un tableau pour commencer.",
  headingH1: "Titre 1",
  headingH2: "Titre 2",
  headingPlaceholder: "Titre de section",
  headingLabel: "Texte du titre",
  paraPlaceholder:
    "Écrivez ici. Utilisez la barre d’outils pour insérer des maths en ligne ou un renvoi.",
  paraLabel: "Texte du paragraphe",
  paraInlineMath: "Maths en ligne",
  paraReference: "Référence",
  paraToolbar: "Insérer dans ce paragraphe",
  tableHeaderCell: "En-tête de colonne",
  tableCell: "Cellule",
  tableAddRow: "Ajouter une ligne",
  tableAddColumn: "Ajouter une colonne",
  tableRemoveRow: "Retirer la ligne",
  tableRemoveColumn: "Retirer la colonne",
  tableBlockLabel: "Tableau modifiable",
  codeSearchLanguage: "Rechercher un langage…",
  codeNoLanguage: "Aucun langage correspondant",
  codeCopy: "Copier",
  codeCopied: "Copié",
  codeInputLabel: "Code",
  codePlaceholder: "Collez ou saisissez votre code…",
  codeWrap: "Retour à la ligne",
  refSection: "Section",
  refEquation: "Éq.",
  refTable: "Tableau",
  refFigure: "Figure",
  refBroken: "renvoi rompu",
  refInsert: "Insérer un renvoi",
  refInsertTitle: "Insérer un renvoi",
  refClose: "Fermer",
  refNoneOfKind: "Rien de ce type pour l’instant.",
  refTabEquations: "Équations",
  refTabSections: "Sections",
  refTabTables: "Tableaux",
  refTabFigures: "Figures",
  driveLoadingFile: (name: string) => `Ouverture de ${name}…`,
  driveOpeningEditor: "votre fichier",
  driveFileOpenFailedTitle: "Ce fichier ne s’est pas ouvert",
  driveFileUnavailable:
    "Il a peut-être été déplacé ou supprimé. Revenez à vos fichiers et choisissez un autre élément.",
  driveEditorLoadFailed: (reason: string) =>
    `Drive n’a pas pu ouvrir ce fichier. ${reason}`,
  driveBackToFiles: "Retour aux fichiers",

  // Outils de facturation de l’agent (ADR 0035, B1.25). Chacun produit un
  // brouillon : approuver n’émet rien, ne numérote rien et n’envoie rien.
  agentActInvoiceDraft: "Facture en brouillon",
  agentActQuoteToInvoice: "Accepter le devis",
  agentActPaymentReminder: "Relance de paiement",
  agentFieldCustomer: "Client",
  agentFieldLines: "Lignes",
  agentFieldQuote: "Devis",
  agentFieldInvoice: "Facture",
  agentLineCount: (n: number): string => (n === 1 ? "1 ligne" : `${n} lignes`),
  agentInvoiceDraftNote:
    "Crée un brouillon — rien n’est émis, numéroté ni envoyé.",
  agentQuoteToInvoiceNote:
    "Clôture le devis comme accepté et crée une facture en brouillon.",
  agentReminderNote:
    "Écrit une relance dans vos Brouillons — rien n’est envoyé.",

  // alo Facturation (ADR 0035, vague B1) — clients et tarifs. Le module parle
  // de documents (« établir une facture »), jamais de lignes de base de
  // données, et n’énonce aucune règle de validation qui appartient au serveur :
  // un refus est affiché dans les mots du serveur, pour que les deux ne
  // puissent jamais se contredire.
  moduleBilling: "Facturation",
  billingCustomers: "Clients",
  billingProducts: "Tarifs",
  billingSearchCustomers: "Rechercher un client…",
  billingSearchProducts: "Rechercher dans les tarifs…",
  billingShowArchived: "Afficher les archivés",
  billingArchived: "Archivé",
  billingArchive: "Archiver",
  billingRestore: "Restaurer",
  billingNewCustomer: "Nouveau client",
  billingNewProduct: "Nouvel article",
  billingEditCustomer: "Modifier le client",
  billingEditProduct: "Modifier l’article",
  billingCustomerSubtitle:
    "La personne ou l’entreprise au nom de qui vos factures sont établies.",
  billingProductSubtitle:
    "Un article que vous pouvez choisir en établissant un document.",
  billingArchiveCustomerConfirm: (name: string) =>
    `Archiver ${name} ? Ce client disparaît des listes de choix ; tous les documents déjà établis continuent de le nommer.`,
  billingArchiveProductConfirm: (name: string) =>
    `Archiver ${name} ? Cet article disparaît des listes de choix ; les documents déjà établis gardent le prix auquel ils ont été établis.`,
  billingCreate: "Créer",
  billingSave: "Enregistrer",
  billingCancel: "Annuler",
  billingLoadFailed:
    "Impossible de charger cette liste. Vérifiez votre connexion et réessayez.",
  billingLoading: "Chargement des données de facturation…",
  billingSaveFailed:
    "Impossible d’enregistrer. Vérifiez votre connexion et réessayez.",
  billingNoMatches: "Aucun résultat pour cette recherche.",
  billingNoCustomersTitle: "Aucun client pour l’instant",
  billingGetStarted: "Démarrez en 3 étapes simples",
  billingStepCustomerTitle: "Ajoutez votre premier client",
  billingStepCustomerBody: "Créez un profil client avec ses informations de facturation.",
  billingStepInvoiceTitle: "Créez votre première facture",
  billingStepInvoiceBody: "Ajoutez des articles, définissez les modalités et émettez la facture.",
  billingStepPaidTitle: "Soyez payé plus rapidement",
  billingStepPaidBody: "Enregistrez les paiements et suivez votre trésorerie.",
  billingNoCustomersBody:
    "Un client porte l’adresse, le numéro de TVA et le délai de paiement dont part chaque facture que vous établissez pour lui.",
  billingNoProductsTitle: "Vos tarifs sont vides",
  billingNoProductsBody:
    "Enregistrez une fois ce que vous vendez, puis choisissez-le en établissant un devis ou une facture.",
  billingColName: "Nom",
  billingColLocation: "Lieu",
  billingColVatId: "Numéro de TVA",
  billingColEmail: "Courriel",
  billingColTerms: "Délai de paiement",
  billingColCurrency: "Devise",
  billingColUnit: "Unité",
  billingColUnitPrice: "Prix unitaire",
  billingColVatRate: "Taux de TVA",
  billingColActions: "Actions",
  billingTermsDays: (days: number) => (days === 1 ? "1 jour" : `${days} jours`),
  billingFieldName: "Nom",
  billingFieldEmail: "Courriel de facturation",
  billingFieldAddress: "Adresse",
  billingFieldAddress2: "Adresse, deuxième ligne",
  billingFieldPostalCode: "Code postal",
  billingFieldCity: "Ville",
  billingFieldCountry: "Pays",
  billingFieldVatId: "Numéro de TVA",
  billingFieldTerms: "Délai de paiement (jours)",
  billingFieldCurrency: "Devise",
  billingFieldUnit: "Unité",
  billingFieldUnitPrice: "Prix unitaire",
  billingFieldVatRate: "Taux de TVA (%)",
  billingEmailPlaceholder: "facturation@exemple.fr",
  billingAddressPlaceholder: "Numéro et rue",
  billingCountryPlaceholder: "BE",
  billingCountryHint: "Code pays à deux lettres.",
  billingCurrencyPlaceholder: "EUR",
  billingVatIdPlaceholder: "BE0123456789",
  billingVatIdHint: "Laissez vide pour un particulier.",
  billingTermsPlaceholder: "30",
  billingTermsHint: "Jours entre l’émission et l’échéance.",
  billingUnitPlaceholder: "heure",
  billingUnitHint: "Le nom d’une unité. Laissez vide pour un forfait.",
  billingAmountPlaceholder: "0,00",
  billingPriceHint: "Hors TVA.",
  billingRatePlaceholder: "20",
  billingRateHint: "0 pour un article exonéré.",
  billingNotAnAmount: "Saisissez un montant, par exemple 1250,00.",
  billingNotARate: "Saisissez un taux, par exemple 20.",

  // Factures (B1.14) : la liste et l’éditeur de brouillon. Chaque chiffre lu
  // ici est celui du serveur ; la formulation ne promet jamais un total
  // calculé par le navigateur, et dit clairement quand un chiffre a une
  // modification de retard.
  billingInvoices: "Factures",
  billingNewInvoice: "Nouvelle facture",
  billingSearchInvoices: "Rechercher par numéro, client ou référence…",
  billingFilterStatus: "Afficher",
  billingFilterAll: "Tous les documents",
  billingStatusDraft: "Brouillon",
  billingStatusIssued: "Émise",
  billingStatusPaid: "Réglée",
  billingStatusVoid: "Annulée",
  billingStatusOverdue: "En retard",
  billingCreditNote: "Avoir",
  billingCreditNotes: "Avoirs",
  billingNoInvoicesTitle: "Aucune facture pour l’instant",
  billingNoInvoicesBody:
    "Établissez un brouillon pour un client, ajoutez ce que vous lui facturez, puis émettez-le quand il est juste.",
  billingColNumber: "Numéro",
  billingColCustomer: "Client",
  billingColIssueDate: "Date d’émission",
  billingColDueDate: "Échéance",
  billingColStatus: "Statut",
  billingColTotal: "Total",
  billingColDescription: "Désignation",
  billingColQty: "Quantité",
  billingColNet: "Montant HT",
  billingNotNumbered: "—",
  billingNoDate: "—",
  billingUnknownCustomer: "Client inconnu",
  billingDraftInvoice: "Facture en brouillon",
  billingBackToInvoices: "Toutes les factures",
  billingInvoiceGone: "Ce document n’existe plus.",
  billingFieldCustomer: "Client",
  billingChooseCustomer: "Choisissez un client…",
  billingCustomerFixedHint:
    "Sa devise et son délai de paiement sont recopiés sur le document.",
  billingFieldReference: "Sa référence",
  billingReferencePlaceholder: "BC-1234",
  billingReferenceHint:
    "Le numéro de commande du client, imprimé sur le document.",
  billingFieldNote: "Note",
  billingNotePlaceholder: "Ce que le client doit lire sur le document.",
  billingNoteHint: "Imprimée sous les lignes.",
  billingFieldIssueDate: "Date d’émission",
  billingFieldDueDate: "Échéance",
  billingCreateDraft: "Créer le brouillon",
  billingCreateDraftHint:
    "Le brouillon est établi d’abord ; vous ajoutez ensuite ce que vous facturez.",
  billingLines: "Lignes",
  billingAddLine: "Ajouter une ligne",
  billingRemoveLine: "Retirer cette ligne",
  billingNoLines: "Rien sur ce document pour l’instant.",
  billingPickProduct: "Depuis les tarifs…",
  billingDescriptionPlaceholder: "Ce que vous facturez",
  billingQtyPlaceholder: "1",
  billingLineNeedsDescription:
    "Une ligne a besoin d’une désignation avant que le brouillon puisse être enregistré.",
  billingNotAQuantity: "Saisissez une quantité, par exemple 1,5.",
  billingTotalsNet: "Total HT",
  billingTotalsGross: "Total TTC",
  billingVatAtRate: (rate: string) => `TVA à ${rate}`,
  billingTotalsStale:
    "Ce sont les derniers chiffres envoyés par le serveur ; ils sont mis à jour à l’enregistrement du brouillon.",
  billingSaving: "Enregistrement…",
  billingSaved: "Enregistré",
  billingUnsaved: "Pas encore enregistré",
  billingSaveNotDone: "Enregistrement impossible",
  billingSaveNow: "Réessayer",
  billingDeleteDraft: "Supprimer le brouillon",
  billingDeleteDraftConfirm:
    "Supprimer ce brouillon ? Il ne porte aucun numéro, donc il ne laisse rien derrière lui — et rien ne pourra être récupéré.",
  billingFrozenNotice:
    "Ce document porte un numéro et ne peut plus être modifié. Corrigez-le par un avoir.",

  // Cycle de vie (B1.15). Chacune de ces actions est irréversible sur un
  // document légal : la confirmation dit ce qu’elle VA FAIRE — consommer un
  // numéro, figer les prix, clore l’offre — plutôt que de demander si l’on est
  // sûr. Aucune ne promet de courriel.
  billingActionFailed:
    "Cela n’a pas abouti. Vérifiez votre connexion et réessayez.",
  billingActionsWaitForSave:
    "Ces actions attendent l’enregistrement de votre dernière modification.",
  billingIssue: "Émettre",
  billingIssueTitle: "Émettre cette facture ?",
  billingIssueConfirm:
    "L’émission consomme le numéro suivant de votre série, date le document et le fige. Il ne pourra plus jamais être modifié — une erreur se corrige ensuite par un avoir. Rien n’est envoyé par courriel au client.",
  billingVoid: "Annuler",
  billingVoidTitle: "Annuler cette facture ?",
  billingVoidConfirm:
    "Une facture annulée garde son numéro et reste lisible, mais ne vaut plus rien. N’annulez qu’un document que personne n’a vu ; si le client détient déjà celui-ci, établissez plutôt un avoir.",
  billingVoidNotice:
    "Cette facture a été annulée. Elle garde son numéro et ne vaut plus rien.",
  billingCreditNoteAction: "Avoir",
  billingCreditNoteTitle: "Établir un avoir ?",
  billingCreditNoteConfirm:
    "Ceci crée un avoir en brouillon reprenant chaque ligne de cette facture, au négatif. Réduisez-le pour un avoir partiel, puis émettez-le comme tout autre document.",
  billingCreditsInvoice: "La facture que cet avoir corrige",
  billingFromQuote: "Le devis dont ce document est issu",

  // Règlements (B1.19) : l’argent reçu contre une facture. Chaque chiffre est
  // celui du serveur, et « partiellement réglée » n’est délibérément jamais
  // appelé un statut : le document reste émis, reste dû, et reste en retard
  // une fois sa date passée.
  billingPayments: "Règlements",
  billingRecordPayment: "Enregistrer un règlement",
  billingRecordPaymentHint:
    "De l’argent qui est arrivé. Rien n’est envoyé nulle part : ceci ne fait qu’enregistrer ce que votre banque montre déjà.",
  billingRemovePayment: "Retirer",
  billingNoPayments: "Rien n’a encore été reçu contre cette facture.",
  billingPaidToDate: "Reçu",
  billingOutstanding: "Reste dû",
  billingOverpaidNote:
    "Il a été reçu plus que cette facture ne vaut. La différence est à vous de rembourser ou de créditer sur la suivante.",
  billingPaymentUnpaid: "Non réglée",
  billingPaymentPartiallyPaid: "Partiellement réglée",
  billingPaymentPaid: "Soldée",
  billingColPaidOn: "Reçu le",
  billingColMethod: "Moyen",
  billingColPaymentReference: "Référence bancaire",
  billingColAmount: "Montant",
  billingFieldAmount: (currency: string) => `Montant (${currency})`,
  billingFieldAmountHint:
    "Ce qui est réellement arrivé, qui peut être moins que la facture.",
  billingFieldPaidOn: "Reçu le",
  billingFieldPaidOnHint:
    "Le jour indiqué par votre banque. Laissez vide pour aujourd’hui.",
  billingFieldMethod: "Comment il est arrivé",
  billingFieldMethodHint: "Texte libre — le mot qu’emploie votre comptabilité.",
  billingMethodPlaceholder: "Virement",
  billingFieldPaymentReference: "Référence bancaire",
  billingFieldPaymentRefHint:
    "La référence de la ligne de relevé, pour pouvoir la rapprocher plus tard.",
  billingFilterOverdue: "En retard",
  billingColOutstanding: "Reste dû",

  // Récapitulatif de TVA d’une période (B1.20) : les chiffres depuis lesquels
  // une déclaration est recopiée. La formulation dit clairement quels
  // documents sont comptés, parce qu’une personne répond légalement de ce
  // qu’elle recopie depuis cet écran.
  billingReports: "Récapitulatif de TVA",
  billingReportFrom: "Du",
  billingReportTo: "Au",
  billingReportShow: "Afficher",
  billingReportThisQuarter: "Ce trimestre",
  billingReportLastQuarter: "Trimestre précédent",
  billingReportDownloadCsv: "Télécharger le CSV",
  billingReportDownloadFailed: "Le fichier n’a pas pu être préparé. Réessayez.",
  billingReportBasis: (from: string, to: string) =>
    `Documents émis et réglés, datés du ${from} au ${to}. Les avoirs sont soustraits ; les brouillons et les documents annulés ne sont pas comptés.`,
  billingReportColVat: "TVA",
  billingReportTotal: "Total",
  billingReportGross: "TTC",
  billingReportCaption: (currency: string) =>
    `Récapitulatif de TVA en ${currency}`,
  billingReportCounts: (invoices: number, creditNotes: number) =>
    `À partir de ${invoices} facture${invoices === 1 ? "" : "s"} et ${creditNotes} avoir${
      creditNotes === 1 ? "" : "s"
    }.`,
  billingReportEmptyTitle: "Rien n’a été émis sur cette période",
  billingReportEmptyBody:
    "Un document compte à partir du jour où il a été émis. Choisissez une autre période, ou émettez les brouillons qui appartiennent à celle-ci.",

  // Devis (B1.15) : le même document qu’une facture jusqu’à ce que quelqu’un
  // dise oui, et délibérément les mêmes mots partout où les deux écrans
  // s’accordent.
  billingQuotes: "Devis",
  billingNewQuote: "Nouveau devis",
  billingSearchQuotes: "Rechercher par numéro, client ou référence…",
  billingNoQuotesTitle: "Aucun devis pour l’instant",
  billingNoQuotesBody:
    "Proposez un prix à un client. S’il accepte, le devis devient une facture en brouillon avec les mêmes lignes.",
  billingQuoteStatusSent: "Envoyé",
  billingQuoteStatusAccepted: "Accepté",
  billingQuoteStatusDeclined: "Refusé",
  billingQuoteStatusExpired: "Expiré",
  billingQuoteLapsed: "Date dépassée",
  billingColSentDate: "Envoyé le",
  billingColValidUntil: "Valable jusqu’au",
  billingDraftQuote: "Devis en brouillon",
  billingBackToQuotes: "Tous les devis",
  billingQuoteGone: "Ce devis n’existe plus.",
  billingQuoteCustomerHint: "Sa devise est recopiée sur l’offre.",
  billingCreateQuoteHint:
    "Le brouillon est établi d’abord ; vous ajoutez ensuite ce que vous proposez.",
  billingFieldSentDate: "Envoyé le",
  billingFieldValidUntil: "Valable jusqu’au",
  billingValidForDays: (days: number) =>
    days === 1
      ? "Valable 1 jour à compter de son envoi."
      : `Valable ${days} jours à compter de son envoi.`,
  billingDeleteQuoteDraft: "Supprimer le brouillon",
  billingDeleteQuoteDraftConfirm:
    "Supprimer ce brouillon ? Il ne porte aucun numéro et n’a jamais été proposé à personne — et rien ne pourra être récupéré.",
  billingQuoteSentNotice:
    "Cette offre a été envoyée et ne peut plus être modifiée. Si le prix change, faites un nouveau devis.",
  billingQuoteClosedNotice:
    "Cette offre est close et ne peut plus être modifiée.",
  billingSendQuote: "Marquer comme envoyé",
  billingSendQuoteTitle: "Envoyer ce devis ?",
  billingSendQuoteConfirm:
    "Ceci consomme le numéro de devis suivant, date l’offre et fige ses prix, pour que ce que détient le client ne puisse pas changer sous ses yeux. Rien n’est envoyé par courriel — envoyez-le vous-même et notez-le ici.",
  billingAcceptQuote: "Accepté",
  billingAcceptQuoteTitle: "Le client a accepté ?",
  billingAcceptQuoteConfirm:
    "Ceci clôt l’offre et établit une facture en brouillon avec les mêmes lignes aux mêmes prix. Rien n’est encore émis — vous arriverez sur le brouillon.",
  billingDeclineQuote: "Refusé",
  billingDeclineQuoteTitle: "Le client a refusé ?",
  billingDeclineQuoteConfirm:
    "L’offre se clôt définitivement et reste lisible. Un changement d’avis est un nouveau devis, pas une offre rouverte.",
  billingExpireQuote: "Y renoncer",
  billingExpireQuoteTitle: "Cesser de relancer cette offre ?",
  billingExpireQuoteConfirm:
    "L’offre se clôt comme expirée, avec la date du jour comme jour où vous avez cessé de la relancer. Elle ne pourra plus recevoir de réponse.",
  billingQuoteInvoice: "La facture que ce devis est devenu",

  // Impression, et l’identité de l’émetteur que porte chaque document imprimé
  // (B1.16). Le document lui-même est rendu par le serveur et parle sa propre
  // table de langue (`billing_print.rs`) ; voici les mots autour de lui.
  billingPrint: "Imprimer",
  billingPrintUnsaved:
    "Ceci imprime le document enregistré : il attend donc votre dernière modification.",
  billingPrintFailed:
    "Le document n’a pas pu être préparé pour l’impression. Réessayez.",
  billingSettings: "Vos coordonnées",
  billingSettingsIntro:
    "Voici de qui viennent vos factures, avoirs et devis : le nom et les numéros en haut, et le compte sur lequel l’argent arrive.",
  billingSettingsFirstRun:
    "Remplissez ceci avant d’émettre quoi que ce soit. C’est ce qui apparaît en haut de chaque document que vous imprimez, et l’endroit où vos clients sont invités à payer.",
  billingSettingsIdentity: "Au nom de qui vous facturez",
  billingSettingsContact: "Comment vos clients vous joignent",
  billingSettingsBank: "Où va l’argent",
  billingSettingsFooter: "La ligne sous les totaux",
  billingSettingsSaved:
    "Enregistré. Chaque document imprimé désormais porte ceci.",
  billingSettingsLoadFailed:
    "Vos coordonnées de facturation n’ont pas pu être chargées.",
  billingFieldLegalName: "Raison sociale",
  billingLegalNameHint:
    "Le nom sous lequel vous exercez et facturez, tel qu’immatriculé.",
  billingIssuerVatIdHint:
    "Laissez vide si vous n’êtes pas assujetti à la TVA. Indiquez d’abord votre pays.",
  billingFieldRegistrationNo: "Numéro d’immatriculation",
  billingRegistrationHint:
    "Tel que l’imprime votre registre — SIREN, KVK, HRB, Companies House.",
  billingFieldPhone: "Téléphone",
  billingFieldWebsite: "Site web",
  billingFieldIban: "IBAN",
  billingIbanHint:
    "Vérifié contre la longueur de votre pays et ses chiffres de contrôle avant l’enregistrement.",
  billingIbanPlaceholder: "BE68 5390 0754 7034",
  billingFieldBic: "BIC",
  billingBicPlaceholder: "KREDBEBB",
  billingBicHint: "Le code BIC ou SWIFT international de votre banque.",
  billingFieldBankName: "Banque",
  billingFieldAccountHolder: "Titulaire du compte",
  billingAccountHolderHint:
    "Uniquement si le compte n’est pas à votre raison sociale.",
  billingFieldFooterNote: "Mention de pied de page",
  billingFooterNoteHint:
    "Imprimée sous les totaux de chaque document — réserve de propriété, pénalités de retard, un remerciement.",

  // Multidevise (B1.21). La formulation est précise sur deux points dont une
  // personne répond légalement : la devise dans laquelle les comptes sont
  // tenus, et le fait qu’un total converti n’est complet que si chaque
  // document qu’il contient a pu être converti.
  billingSettingsAccounting: "La devise dans laquelle vous tenez vos comptes",
  billingFieldBaseCurrency: "Devise comptable",
  billingBaseCurrencyHint:
    "Vous pouvez facturer dans n’importe quelle devise. Celle-ci est celle dans laquelle votre déclaration de TVA est déposée, et dans laquelle la TVA d’une facture en devise étrangère est également imprimée.",
  billingFxRates: "Taux de change",
  billingFxIntro:
    "Facturer dans une autre devise demande le taux publié du jour de l’émission. Les taux sont les vôtres : rien n’est récupéré pour vous, donc le taux auquel vos comptes sont convertis vient d’un fichier que vous avez choisi.",
  billingFxColDate: "Publié le",
  billingFxColRate: "Taux pour un euro",
  billingFxColSource: "Origine",
  billingFxSourceEcb: "Fichier de référence",
  billingFxSourceManual: "Saisi à la main",
  billingFxAdd: "Ajouter un taux",
  billingFxAddSaved: (currency: string, date: string) =>
    `Taux ${currency} du ${date} enregistré.`,
  billingFxRateHint:
    "Tel que publié : unités de cette devise pour un euro, écrit 1,1626.",
  billingFxImport: "Importer un fichier de taux",
  billingFxImportHint:
    "Collez le CSV eurofxref de la Banque centrale européenne, ou tout fichier de cette forme. Un fichier contenant une seule valeur incorrecte ne change rien.",
  billingFxImportRun: "Importer",
  billingFxImported: (rates: number, days: number) =>
    `${rates} taux importés sur ${days} jours.`,
  billingFxEmpty:
    "Aucun taux pour l’instant. Vous n’en avez besoin que si vous facturez dans une autre devise.",
  billingFxLoadFailed: "Les taux de change n’ont pas pu être chargés.",
  billingDocumentFx: (rate: string, day: string) =>
    `Converti à ${rate}, taux de référence publié le ${day}.`,
  billingVatIn: (currency: string) => `TVA en ${currency}`,
  billingReportBaseCaption: (currency: string) => `La période en ${currency}`,
  billingReportBaseIntro: (currency: string) =>
    `Chaque document ci-dessus, converti au taux figé sur lui à son émission. C’est depuis ceci qu’une déclaration en ${currency} est établie.`,
  billingReportUnconverted: (count: number) =>
    count === 1
      ? "1 document n’est pas dans ces chiffres : aucun taux de change n’a été enregistré pour lui. Vérifiez-le avant de déclarer."
      : `${count} documents ne sont pas dans ces chiffres : aucun taux de change n’a été enregistré pour eux. Vérifiez-les avant de déclarer.`,

  // Relancer un impayé (B1.26). La formulation tient surtout à une chose :
  // ceci écrit une lettre, elle ne l’envoie pas.
  billingRemind: "Relancer",
  billingRemindHint:
    "Écrire une relance de paiement à ce client, et la laisser dans vos Brouillons.",
  billingReminderDrafted: (
    invoice: string,
    outstanding: string,
    days: number,
  ) =>
    days === 1
      ? `Une relance pour ${invoice} — ${outstanding} restant dû, 1 jour après l’échéance — attend dans vos Brouillons. Rien n’a été envoyé : lisez-la, changez ce que vous voulez, et envoyez-la vous-même.`
      : `Une relance pour ${invoice} — ${outstanding} restant dû, ${days} jours après l’échéance — attend dans vos Brouillons. Rien n’a été envoyé : lisez-la, changez ce que vous voulez, et envoyez-la vous-même.`,
  billingReminderFailed:
    "La relance n’a pas pu être écrite. Vérifiez votre connexion et réessayez.",
  billingNothingOverdue:
    "Rien n’est en retard. Chaque facture émise est soit soldée, soit encore dans les délais.",

  // Factures récurrentes (B2.11). Le mot qui compte partout ici : brouillon.
  // Une échéance produit un document à vérifier, jamais une facture émise.
  billingRecurring: "Récurrent",
  billingRecurringTitle: "Factures récurrentes",
  billingRecurringChip: "Récurrente",
  billingRecurringChipHint: "Une facture récurrente a produit ce brouillon.",
  billingNoSchedulesTitle: "Aucune facture récurrente",
  billingNoSchedulesBody:
    "Créez-en une pour tout ce que vous facturez à intervalle régulier — un forfait, un abonnement, un hébergement. À chaque échéance, alo prépare un brouillon que vous vérifiez et émettez vous-même.",
  billingNewSchedule: "Nouvelle facture récurrente",
  billingScheduleFrom: "Répéter cette facture",
  billingScheduleFromHint:
    "Créez une facture récurrente qui refacturera ces lignes à intervalle régulier. Chaque occurrence apparaît en brouillon — rien n’est jamais émis à votre place.",
  billingScheduleName: "Nom",
  billingScheduleNameHint:
    "Le nom que vous lui donnez. Jamais imprimé sur la facture.",
  billingScheduleCadence: "Fréquence",
  billingCadenceWeekly: "Chaque semaine",
  billingCadenceMonthly: "Chaque mois",
  billingCadenceQuarterly: "Chaque trimestre",
  billingCadenceYearly: "Chaque année",
  billingScheduleStart: "Première le",
  billingScheduleEnd: "Jusqu’au",
  billingScheduleEndNever: "Sans date de fin",
  billingScheduleNext: "Prochaine",
  billingScheduleLast: "Dernière produite",
  billingScheduleRaised: "Produite",
  billingScheduleEach: "À chaque fois",
  billingScheduleStatusActive: "En cours",
  billingScheduleStatusPaused: "En pause",
  billingScheduleStatusEnded: "Terminée",
  billingScheduleStatusDue: "À échéance",
  billingSchedulePause: "Mettre en pause",
  billingScheduleResume: "Reprendre",
  billingScheduleDelete: "Supprimer",
  billingScheduleDeleteTitle: "Supprimer cette facture récurrente ?",
  billingScheduleDeleteMessage:
    "Elle cessera de facturer et disparaîtra de cette liste. Seule une facture récurrente n’ayant jamais produit de brouillon peut être supprimée — mettez en pause celle qui en a produit.",
  billingScheduleRunDue: "Produire ce qui est dû",
  billingScheduleRunHint:
    "alo le fait tout seul chaque heure. Ceci n’est là que si vous préférez ne pas attendre.",
  billingScheduleRunNone:
    "Rien n’était dû. Toutes vos factures récurrentes sont à jour.",
  billingScheduleRunDrafted: (count: number) =>
    count === 1
      ? "1 brouillon a été produit et attend dans vos factures. Rien n’a été émis : lisez-le, changez ce que vous voulez, et émettez-le vous-même."
      : `${count} brouillons ont été produits et attendent dans vos factures. Rien n’a été émis : lisez-les, changez ce que vous voulez, et émettez-les vous-même.`,
  billingScheduleSaved: (name: string) =>
    `« ${name} » est en place. À chaque échéance, alo produira un brouillon que vous pourrez vérifier.`,
  billingScheduleAnchorHint: (day: number) =>
    day > 28
      ? `Calée sur le ${day} : dans un mois plus court, elle facture le dernier jour, puis de nouveau le ${day} au mois suivant assez long.`
      : `Calée sur le ${day === 1 ? "1er" : day} du mois.`,

  // alo CRM (B2). Le vocabulaire commercial français : une « affaire »
  // (deal) avance par « étapes » (stages) sur un « tableau » (board), et se
  // clôt gagnée ou perdue. « Pipeline » est passé dans l’usage : il reste.
  moduleCrm: "Ventes",
  crmBoard: "Tableau",
  crmList: "Liste",
  crmPipeline: "Pipeline",
  crmDeal: "Affaire",
  crmStage: "Étape",
  crmStageArchived: "Colonne archivée",
  crmLoadFailed: "Vos affaires n’ont pas pu être chargées.",
  crmSaveFailed: "La modification n’a pas pu être enregistrée.",
  crmDeleteFailed: "Cet élément n’a pas pu être supprimé.",
  crmSuggestFailed: "Aucune conversation n’a pu être proposée pour le moment.",
  crmNoBoardTitle: "Aucun pipeline",
  crmNoBoardBody:
    "Tous vos tableaux ont été archivés. Restaurez-en un pour travailler à nouveau vos affaires.",
  crmNoDealsTitle: "Aucune affaire",
  crmNoDealsBody:
    "Créez la première opportunité et faites-la avancer sur le tableau.",
  crmNoMatches: "Aucune affaire ne correspond à votre recherche.",

  // Le formulaire d’affaire
  crmNewDeal: "Nouvelle affaire",
  crmEditDeal: "Modifier l’affaire",
  crmEdit: "Modifier",
  crmCreate: "Créer",
  crmSave: "Enregistrer",
  crmCancel: "Annuler",
  crmClose: "Fermer",
  crmDealSubtitle:
    "Ce qu’est l’opportunité, avec qui elle se joue, et ce qu’elle vaut.",
  crmFieldTitle: "Affaire",
  crmFieldCompany: "Société",
  crmCompanyHint: "La société telle que toute votre équipe doit la voir.",
  crmFieldContactName: "Contact",
  crmFieldContactEmail: "E-mail du contact",
  crmContactEmailHint:
    "Sert à proposer les conversations auxquelles cette affaire se rattache.",
  crmFieldValue: "Montant",
  crmValueHint: "Ce que vaut l’affaire, hors TVA.",
  crmFieldCurrency: "Devise",
  crmCurrencyHint: "Trois lettres, par exemple EUR.",
  crmFieldExpectedClose: "Clôture prévue",
  crmFieldSource: "Origine",
  crmSourceHint:
    "D’où vient l’opportunité — une recommandation, une campagne, un appel.",
  crmNotAnAmount: "Ce n’est pas un montant.",
  crmDeleteDeal: "Supprimer",
  crmDeleteDealConfirm:
    "Ceci supprime l’affaire et tout ce qui y est consigné. Les tâches qui en sont issues restent dans les listes de leurs responsables. C’est irréversible.",

  // La liste
  crmDealsTable: "Affaires",
  crmDealFilters: "Filtres des affaires",
  crmSearchDeals: "Rechercher une affaire",
  crmFilterStage: "Filtrer par étape",
  crmFilterAnyStage: "Toutes les étapes",
  crmFilterState: "Filtrer par état",
  crmFilterAnyState: "Tous les états",
  crmFilterMine: "Seulement les miennes",
  crmColDeal: "Affaire",
  crmColCompany: "Société",
  crmColStage: "Étape",
  crmColValue: "Montant",
  crmColExpectedClose: "Clôture prévue",
  crmColState: "État",
  crmStateOpen: "En cours",
  crmStateWon: "Gagnée",
  crmStateLost: "Perdue",
  crmExpectedClose: (day: string) => `Prévue le ${day}`,
  crmLostBecause: (reason: string) => `Perdue : ${reason}`,

  // Perdre une affaire demande pourquoi : une raison facultative est une
  // raison que personne ne saisit — et le rapport gagné/perdu en vit.
  crmLostTitle: "Pourquoi a-t-elle été perdue ?",
  crmLostMessage: (stage: string) =>
    `Déplacer cette affaire vers « ${stage} » la clôt comme perdue. Dites pourquoi, afin que la raison figure dans votre rapport gagné/perdu.`,
  crmLostPlaceholder: "Prix, calendrier, partie chez un concurrent…",
  crmLostConfirm: "Marquer comme perdue",
  crmLostReasonLabel: "Raison",
  crmLostReasonPrice: "Prix",
  crmLostReasonTiming: "Calendrier",
  crmLostReasonCompetitor: "A choisi un concurrent",
  crmLostReasonBudget: "Pas de budget",
  crmLostReasonNoDecision: "Pas de décision",
  crmLostReasonNotAFit: "Pas adaptée",

  // Gagner une affaire : le passage à la facturation. Les deux créent un
  // BROUILLON — rien n’est émis, rien n’est envoyé, aucun numéro consommé.
  // « brouillon de facture / de devis » est masculin dans les deux cas :
  // les phrases qui l’interpolent restent grammaticales.
  crmRaiseQuote: "Devis",
  crmRaiseInvoice: "Facture",
  crmDocumentDraft: (kind: string): string =>
    kind === "invoice" ? "brouillon de facture" : "brouillon de devis",
  crmRaiseTitle: (document: string) => `Créer un ${document}`,
  crmRaiseSubtitle:
    "Il arrive dans Facturation en brouillon, à vérifier et à compléter. Rien n’est émis et rien n’est envoyé.",
  crmRaiseFrom: (deal: string, value: string) =>
    `Depuis « ${deal} », d’un montant de ${value}.`,
  crmRaiseConfirm: "Créer",
  crmRaiseFailed: "Le document n’a pas pu être créé.",
  crmFieldVatRate: "Taux de TVA",
  crmVatRateHint:
    "Le taux auquel cette ligne est facturée, en pourcentage — par exemple 21.",
  crmFieldCountry: "Pays du client",
  crmCountryHint:
    "Deux lettres. Cette affaire est encore un prospect : un client en est créé, et le pays détermine le traitement de la TVA.",
  crmRaisedTitle: (document: string) => `Votre ${document} est prêt`,
  crmRaisedSubtitle:
    "Ouvrez-le dans Facturation pour vérifier les lignes, l’adresse et la TVA.",
  crmRaisedWorth: (gross: string) => `${gross} TTC.`,
  crmOpenInBilling: "Ouvrir dans Facturation",

  // Le rapport : le montant par étape, et ce qui a été gagné ou perdu sur
  // une période. Chaque chiffre vient du serveur, et deux devises ne
  // s’additionnent jamais.
  crmReport: "Rapport",
  crmReportPeriod: "Période du rapport",
  crmReportFrom: "Du",
  crmReportTo: "Au",
  crmReportShow: "Afficher",
  crmReportThisQuarter: "Ce trimestre",
  crmReportLastQuarter: "Trimestre précédent",
  crmReportDownloadCsv: "Télécharger le CSV",
  crmReportDownloadFailed: "Le rapport n’a pas pu être téléchargé.",
  crmReportBasis: (from: string, to: string) =>
    `Gagné et perdu entre le ${from} et le ${to}.`,
  crmReportOpenAsOf: (at: string) => `Le pipeline en cours est celui du ${at}.`,
  crmReportOpenCaption: (currency: string) =>
    `Pipeline en cours par étape (${currency})`,
  crmReportClosedCaption: (currency: string) =>
    `Clôturé sur la période (${currency})`,
  crmReportColDeals: "Affaires",
  crmReportOpenTotal: "Total en cours",
  crmReportWinRate: (rate: string, won: number, closed: number) =>
    `Taux de réussite ${rate} — ${won} sur ${closed} affaires clôturées.`,
  crmReportNoWinRate:
    "Aucune affaire n’a été clôturée sur cette période : il n’y a pas de taux de réussite à afficher.",
  crmReportEmptyTitle: "Rien à présenter pour l’instant",
  crmReportEmptyBody:
    "Ce tableau ne contient aucune affaire. Créez-en une et elle apparaîtra ici, par étape et par devise.",

  // Le journal
  crmActivityTitle: "Journal",
  crmActivityKind: "Type d’entrée",
  crmActivityPlaceholder: "Ce qui a été dit ou convenu…",
  crmActivityAdd: "Consigner",
  crmActivityDelete: "Supprimer l’entrée",
  crmActivityEmpty: "Rien n’est encore consigné.",
  crmKindNote: "Note",
  crmKindCall: "Appel",
  crmKindMeeting: "Rendez-vous",

  // Les prochaines étapes sont de vraies tâches, dans la liste que leur
  // responsable ouvre déjà.
  crmNextStepsTitle: "Prochaines étapes",
  crmNextStepPlaceholder: "Ce qui se passe ensuite…",
  crmNextStepDue: "Échéance",
  crmNextStepAdd: "Ajouter",
  crmNextStepsEmpty: "Aucune prochaine étape convenue.",
  crmOpenInTasks: "Ouvrir dans Tâches",

  // Conversations liées. Le courrier reste dans le courrier : le lien est un
  // renvoi, et seul un collègue qui a déjà la conversation peut l’ouvrir.
  crmThreadsTitle: "Conversations",
  crmThreadsEmpty: "Aucune conversation liée.",
  crmThreadSuggest: "Proposer des conversations",
  crmThreadLink: "Lier",
  crmThreadUnlink: "Délier",
  crmThreadOpenInMail: "Ouvrir dans Courrier",
  crmThreadNotYours:
    "Cette conversation n’est pas dans votre boîte — demandez au collègue qui l’a liée.",
  crmThreadLinkedBy: (who: string, when: string) => `Lié par ${who} · ${when}`,
  crmSuggestionsEmpty:
    "Rien dans votre courrier récent ne correspond aux adresses de cette affaire.",
  crmSuggestionAddress: (address: string) => `Correspond à ${address}`,
  crmSuggestionDomain: (address: string) => `Même société que ${address}`,

  // Les propositions de l’agent (ADR 0034) : le cadre commun, puis les
  // actions CRM (B2.10). Rien n’est fait avant l’approbation.
  agentProposedAction:
    "alo souhaite effectuer ceci — approuvez pour continuer.",
  agentApprove: "Approuver",
  agentDiscard: "Écarter",
  agentDone: "C’est fait.",
  agentFailed: "Cette action n’a pas pu être effectuée.",
  agentActCreateDeal: "Nouvelle affaire",
  agentActMoveDeal: "Déplacer l’affaire",
  agentActFollowup: "E-mail de relance",
  agentFieldDeal: "Affaire",
  agentFieldCompany: "Société",
  agentFieldValue: "Montant",
  agentFieldStage: "Étape",
  agentFieldLostReason: "Perdue car",
  agentDealFromEmailNote: "Lie cette conversation à la nouvelle affaire.",
  agentFollowupNote: "Écrit l’e-mail dans vos Brouillons — rien n’est envoyé.",

  // Les actions Projets (B3.10a, B3.10b). Une heure proposée n’est une heure
  // que le jour où la personne concernée l’accepte dans sa feuille de temps :
  // le mot « proposé » revient donc dans chaque phrase, et le point de
  // situation dit qu’il ne fait que lire.
  agentActLogTime: "Saisir des heures",
  agentActProjectStatus: "Point sur le projet",
  agentFieldProject: "Projet",
  agentFieldDay: "Jour",
  agentFieldDuration: "Durée",
  agentLogTimeNote:
    "Propose une saisie dans votre feuille de temps — elle compte une fois que vous l’y acceptez.",
  agentProjectStatusNote: "Lit seulement le projet — rien n’est modifié.",
  // Les chiffres du point de situation. Le serveur envoie des nombres, jamais
  // une phrase : chaque mot lu ici est écrit là.
  agentTimeLogged: (project: string): string =>
    `Proposé dans votre feuille de temps sur ${project} — acceptez-le dans Projets pour qu’il compte.`,
  agentStatusHours: "Heures saisies",
  agentStatusBillable: (formatted: string): string =>
    `dont ${formatted} facturables`,
  agentStatusBudget: "Budget",
  agentStatusBudgetUsed: (percent: string): string => `${percent} consommé`,
  agentStatusNoBudget: "Aucun budget d’heures défini",
  agentStatusInternal: "Projet interne — ni client, ni budget.",
  agentStatusCustomer: "Client",
  agentStatusMilestones: "Jalons",
  agentStatusMilestonesDone: (done: number, total: number): string =>
    `${done} atteints sur ${total}`,
  agentStatusMilestonesLate: (late: number): string =>
    late === 1 ? "1 en retard" : `${late} en retard`,
  agentStatusNoMilestones: "Aucun de prévu",
  agentStatusNext: "Prochain",
  agentStatusTasks: "Tâches",
  agentStatusTasksOpen: (open: number): string =>
    open === 1 ? "1 ouverte" : `${open} ouvertes`,
  agentStatusTasksOverdue: (overdue: number): string => `${overdue} en retard`,
  agentStatusLastWorked: "Dernier travail",
  agentStatusNeverWorked: "Aucune heure",
  // Le brouillon d’agenda (B3.10b). Un lot de propositions, et ce qui en a été
  // écarté — le serveur envoie des codes de motif, et chaque mot est écrit ici.
  agentActDraftTimesheet: "Feuille de temps depuis votre agenda",
  agentDraftTimesheetNote:
    "Propose une saisie par réunion inscrite à votre agenda ces jours-là — chacune compte une fois acceptée dans Projets.",
  agentDraftedCount: (count: number): string =>
    count === 1 ? "1 saisie proposée" : `${count} saisies proposées`,
  agentDraftedNone: "Rien à proposer",
  agentDraftedRange: (from: string, to: string): string =>
    from === to ? from : `${from} – ${to}`,
  agentDraftedTotal: "Total",
  agentDraftedOverlap: "chevauche la précédente",
  agentDraftedOverlaps: (count: number): string =>
    count === 1
      ? "1 d’entre elles en chevauche une autre — vérifiez laquelle était le travail."
      : `${count} d’entre elles en chevauchent d’autres — vérifiez lesquelles étaient le travail.`,
  agentDraftedNote: (project: string): string =>
    `Proposé dans votre feuille de temps sur ${project} — acceptez chaque saisie dans Projets pour qu’elle compte.`,
  agentDraftedLeftOut: "Écarté",
  agentDraftedReason: (reason: string): string => {
    switch (reason) {
      case "allDay":
        return "journée entière — pas des heures travaillées";
      case "alreadyDrafted":
        return "déjà dans votre feuille de temps";
      case "noDuration":
        return "sans durée";
      case "tooLong":
        return "plus longue qu’une journée";
      case "weekLocked":
        return "cette semaine est soumise";
      case "limitReached":
        return "au-delà du lot — redemandez pour les jours restants";
      case "outsideRange":
        return "commence en dehors de ces jours";
      default:
        // Un motif qu’un serveur plus récent connaît et pas ce client : dire
        // qu’il a été écarté plutôt que de faire croire qu’il a été proposé.
        return "écarté";
    }
  },

  // L’historique d’un enregistrement (B2.13). L’anglais emploie des
  // participes (« Issued ») ; en français, un participe s’accorde avec un
  // sujet que la ligne ne nomme pas. Ces libellés sont donc des noms
  // d’action — invariables, et lisibles sur n’importe quel enregistrement.
  auditHistoryTitle: "Historique",
  auditHistoryEmpty: "Rien n’est encore arrivé à cet enregistrement.",
  auditLoadFailed: "L’historique n’a pas pu être chargé.",
  auditActionCreate: "Création",
  auditActionUpdate: "Modification",
  auditActionDelete: "Suppression",
  auditActionArchive: "Archivage",
  auditActionIssue: "Émission",
  auditActionVoid: "Annulation",
  auditActionCreditNote: "Création d’un avoir",
  auditActionSend: "Rédaction d’un e-mail",
  auditActionReminder: "Rédaction d’une relance",
  auditActionPaymentCreate: "Enregistrement d’un paiement",
  auditActionPaymentDelete: "Suppression d’un paiement",
  auditActionImport: "Import",
  auditActionSepaXml: "Ajout à un fichier de paiement",
  auditActionApprove: "Approbation",
  auditActionReject: "Rejet",
  auditActionAccept: "Acceptation",
  auditActionDecline: "Refus",
  auditActionExpire: "Passage en expiré",
  auditActionRun: "Exécution",
  auditActionPause: "Mise en pause",
  auditActionResume: "Reprise",
  auditActionRatesUpdate: "Enregistrement d’un taux de change",
  auditActionRatesImport: "Import de taux de change",
  auditActionStageMove: "Changement de colonne",
  auditActionStageCreate: "Ajout d’une colonne",
  auditActionMove: "Déplacement",
  auditActionQuoteRaised: "Création d’un devis",
  auditActionInvoiceRaised: "Création d’une facture",
  auditActionActivityCreate: "Ajout d’une note",
  auditActionNextStepCreate: "Ajout d’une prochaine étape",
  auditActionThreadCreate: "Liaison d’une conversation",
  auditActionThreadDelete: "Retrait d’une conversation",
  auditActionLeadCreate: "Import de prospects",

  // alo Analyses (ADR 0037, vague BI-1). Un « board » est ici un tableau de
  // bord, jamais un tableau tout court : le mot doit rester le même de la
  // liste des onglets au dialogue de suppression. Les sept questions de
  // l’aperçu reprennent mot pour mot les titres semés par le serveur
  // (`insights_gallery.rs`) — un graphique ne change pas de nom selon qu’il
  // vient de l’aperçu ou de la galerie.
  moduleInsights: "Analyses",
  insightsBoards: "Tableaux de bord",
  insightsLoadFailed: "Vos tableaux de bord n’ont pas pu être chargés.",
  insightsBoardLoadFailed: "Ce tableau de bord n’a pas pu être chargé.",
  insightsFiguresFailed: "Ces chiffres n’ont pas pu être lus.",
  insightsSaveFailed: "La modification n’a pas pu être enregistrée.",
  insightsDeleteFailed: "Cet élément n’a pas pu être retiré.",
  insightsNewBoard: "Nouveau tableau de bord",
  insightsBoardNamePrompt: "Comment ce tableau de bord doit-il s’appeler ?",
  insightsBoardNamePlaceholder: "Trésorerie",
  insightsRenameBoard: "Renommer",
  insightsDeleteBoard: "Supprimer le tableau de bord",
  insightsDeleteBoardConfirm: (name: string) =>
    `Supprimer le tableau de bord « ${name} » ? Ses graphiques partent avec lui — les factures et les affaires qu’ils comptent restent.`,
  insightsRefresh: "Actualiser les chiffres",
  insightsNoBoardsTitle: "Aucun tableau de bord",
  insightsNoBoardsBody:
    "Un tableau de bord réunit les chiffres que vous voulez voir d’un coup d’œil : ce que vous avez facturé, ce qu’on vous doit, ce qu’il y a dans le pipeline.",
  insightsNoTilesTitle: "Rien d’épinglé sur ce tableau de bord",
  insightsNoTilesBody:
    "Les graphiques épinglés sur ce tableau de bord apparaissent ici.",
  insightsAddChart: "Ajouter un graphique",
  insightsGalleryTitle: "Graphiques prêts à l’emploi",
  insightsGallerySubtitle:
    "Choisissez-en un pour l’épingler à ce tableau de bord. Vous pourrez le renommer ou le retirer ensuite.",
  insightsGalleryClose: "Fermer",
  insightsGalleryLoadFailed:
    "Les graphiques prêts à l’emploi n’ont pas pu être chargés.",
  insightsGalleryRevenueByMonth: "Chiffre d’affaires par mois",
  insightsGalleryRevenueByMonthBody:
    "Ce que vous avez facturé, mois par mois, sur l’année écoulée — hors TVA.",
  insightsGalleryOutstanding: "Créances en cours",
  insightsGalleryOutstandingBody:
    "Tout ce qui vous est encore dû sur les factures émises, en un seul chiffre.",
  insightsGalleryOverdueAging: "Retards par ancienneté",
  insightsGalleryOverdueAgingBody:
    "Ce qui vous est dû, groupé selon le retard : 0–30, 31–60, 61–90 et plus de 90 jours.",
  insightsGalleryVatByQuarter: "TVA par trimestre",
  insightsGalleryVatByQuarterBody:
    "La TVA facturée par trimestre — la forme dans laquelle une déclaration se dépose.",
  insightsGalleryTopCustomers: "Meilleurs clients",
  insightsGalleryTopCustomersBody:
    "D’où vient le chiffre d’affaires de l’année, les dix premiers d’abord.",
  insightsGalleryPaymentsByMonth: "Paiements reçus",
  insightsGalleryPaymentsByMonthBody:
    "L’argent réellement arrivé, mois par mois, dans la devise dans laquelle il est arrivé.",
  insightsGalleryPipelineByStage: "Pipeline par étape",
  insightsGalleryPipelineByStageBody:
    "La valeur des affaires en cours dans chaque colonne de votre pipeline.",
  insightsGalleryWonThisMonth: "Gagné ce mois-ci",
  insightsGalleryWonThisMonthBody:
    "La valeur des affaires conclues comme gagnées ce mois-ci.",
  insightsGalleryWinRateByQuarter: "Taux de réussite par trimestre",
  insightsGalleryWinRateByQuarterBody:
    "À quelle fréquence une affaire tranchée a été gagnée, trimestre par trimestre.",
  insightsGalleryWonByMonth: "Gagné par mois",
  insightsGalleryWonByMonthBody:
    "La valeur des affaires gagnées, mois par mois sur l’année écoulée.",
  insightsAsk: "Demander un graphique",
  insightsAskSubtitle:
    "Décrivez ce que vous voulez voir. Vous obtenez d’abord le graphique à regarder — rien n’est ajouté à ce tableau de bord tant que vous ne l’épinglez pas.",
  insightsAskLabel: "Votre question",
  insightsAskPlaceholder:
    "Combien avons-nous facturé chaque mois cette année ?",
  insightsAskSubmit: "Demander",
  insightsAskClose: "Fermer",
  insightsAskPreview: "Le graphique proposé",
  insightsAskPin: "Épingler à ce tableau de bord",
  insightsAskDiscard: "Abandonner",
  insightsAskRepaired:
    "La première tentative ne correspondait pas aux données ; elle a été corrigée avant le tracé.",
  insightsAskFailed:
    "Aucun graphique n’a pu être construit à partir de cette question.",
  insightsAskUnavailable:
    "L’assistant n’est pas activé pour cet espace de travail.",
  insightsTileActions: (title: string) => `Options pour ${title}`,
  insightsRenameTile: "Renommer le graphique",
  insightsRenameTilePrompt: "Comment ce graphique doit-il s’appeler ?",
  insightsRemoveTile: "Retirer le graphique",
  insightsRemoveTileConfirm: (title: string) =>
    `Retirer « ${title} » de ce tableau de bord ? Les enregistrements qu’il compte ne sont pas touchés.`,
  insightsWiden: "Élargir",
  insightsNarrow: "Rétrécir",
  insightsMoveLeft: "Déplacer avant",
  insightsMoveRight: "Déplacer après",
  insightsUnreadableTitle: "Créé par une version plus récente d’alo",
  insightsUnreadableBody:
    "La question de ce graphique ne peut pas être lue ici, ses chiffres ne sont donc pas affichés.",
  insightsNoFigures: "Rien à afficher pour cette période.",
  insightsTruncated:
    "Seules les plus grandes catégories sont affichées ; le reste est regroupé sous « Autres ».",
  insightsNoteUnconverted: (count: number) =>
    count === 1
      ? "1 document n’a pas pu être converti dans votre devise comptable et n’est pas compté."
      : `${count} documents n’ont pas pu être convertis dans votre devise comptable et ne sont pas comptés.`,
  insightsColBucket: "Catégorie",
  insightsColValue: "Valeur",
  insightsBucketTotal: "Total",
  insightsBucketOther: "Autres",
  insightsGroupAll: "Tout",
  insightsValueNone: "Aucun",
  insightsValueUnknown: "Inconnu",
  insightsStatusIssued: "Émise",
  insightsStatusPaid: "Réglée",
  insightsOutcomeWon: "Gagnée",
  insightsOutcomeLost: "Perdue",
  insightsOutcomeOpen: "En cours",
  insightsAgeNotDue: "Non échu",
  insightsAge0To30: "0–30 jours",
  insightsAge31To60: "31–60 jours",
  insightsAge61To90: "61–90 jours",
  insightsAge90Plus: "Plus de 90 jours",
  // Les abréviations françaises : trimestre et semaine, pas Q et W.
  insightsQuarter: (quarter: number, year: number) => `T${quarter} ${year}`,
  insightsWeek: (week: number, year: number) => `S${week} ${year}`,

  // alo Projets (ADR 0035, vague B3). Le vocabulaire du travail client : un
  // projet réalisé pour un client, les heures qui y passent, la semaine sous
  // laquelle elles sont remises, et la décision prise sur cette semaine.
  //
  // Deux mots sont fixés une fois pour toutes ici. Une « feuille de temps »
  // est le document que l’on remplit — jamais un « timesheet », jamais un
  // « relevé ». Et une semaine est « validée » ou « renvoyée » : « approuvée »
  // parlerait d’un accord, alors qu’il s’agit d’un contrôle.
  //
  // Les durées s’écrivent comme on les dit — « 7 h 30 » — et jamais en heures
  // décimales : « 1,75 » sur un écran à côté de « 1 h 45 » sur un autre, ce
  // sont deux nombres que quelqu’un doit rapprocher.
  moduleProjects: "Projets",
  projectsTabList: "Tous les projets",
  projectsTabMyWork: "Mon travail",
  projectsWorkspaceTasks: "Tâches",
  projectsTabWeek: "Feuille de temps",
  projectsTabApprovals: "Validations",
  projectsTabReports: "Rapports",
  projectsTabPlan: "Chronologie",
  projectsLoadFailed: "Vos projets n’ont pas pu être chargés.",
  projectsSaveFailed: "La modification n’a pas pu être enregistrée.",
  projectsStartFailed: "Le chronomètre n’a pas pu être démarré.",
  projectsStopFailed: "Le chronomètre n’a pas pu être arrêté.",
  projectsCancel: "Annuler",
  projectsSave: "Enregistrer",
  projectsEdit: "Modifier",
  projectsDetailsTitle: "Détails du projet",
  projectsDetailsSubtitle: "Gardez le résultat, le calendrier et l’état actuel clairs pour tous.",
  projectsDescription: "Description",
  projectsStatus: "Statut",
  projectsStatusPlanned: "Planifié",
  projectsStatusActive: "Actif",
  projectsStatusOnHold: "En pause",
  projectsStatusCompleted: "Terminé",
  projectsStatusCancelled: "Annulé",
  projectsTargetOn: "Date cible",
  projectsDatesInvalid: "La date cible ne peut pas précéder la date de début.",
  projectsActions: "Actions",
  projectsNew: "Nouveau projet",
  projectsNewTitle: "Créer un projet",
  projectsNewSubtitle: "Nommez le travail et indiquez pour qui il est réalisé.",
  projectsName: "Nom du projet",
  projectsNamePlaceholder: "Par exemple, Refonte du site web",
  projectsWorkType: "Ce travail est destiné à",
  projectsClientWork: "Un client",
  projectsInternalWork: "Notre entreprise",
  projectsNewCustomerHint: "Vous pourrez ajouter les tarifs et budgets après la création.",
  projectsCreate: "Créer le projet",
  projectsCreateFailed: "Le projet n’a pas pu être créé.",

  // Durées et taux. `projectsNoTime` est le tiret d’une case vide : une case
  // blanche se lit comme une panne, un zéro comme un travail sans durée.
  projectsNoTime: "—",
  projectsHoursShort: (hours: number) => `${hours} h`,
  projectsMinutesShort: (minutes: number) => `${minutes} min`,
  projectsPerHour: (amount: string) => `${amount}/h`,
  projectsPercent: (percent: number) => `${percent} %`,
  projectsUnpriced: "Non tarifé",

  // La liste des projets.
  projectsProject: "Projet",
  projectsAllProjects: "Tous les projets",
  projectsCustomer: "Client",
  projectsCustomerHint:
    "Le client à qui les heures de ce projet sont facturées.",
  projectsCustomerPick: "Choisissez un client…",
  projectsCustomerUnknown: "Client inconnu",
  projectsInternal: "Interne",
  projectsRate: "Taux horaire",
  projectsRateHint:
    "Laissé vide, les heures sont comptées mais non valorisées.",
  projectsRateInvalid:
    "Écrivez le taux sous forme de montant, par exemple 95,00.",
  projectsHoursLogged: "Heures",
  projectsBillableHours: "Facturables",
  projectsOfWhichBillable: (duration: string) => `dont ${duration} facturables`,
  projectsBudget: "Budget",
  projectsHealth: "Santé du projet",
  projectsHealthOnTrack: "Dans les temps",
  projectsHealthAtRisk: "Attention requise",
  projectsHealthNeedsTarget: "Ajoutez une date cible pour rendre le risque de livraison visible.",
  projectsBlockedTasks: (count: number) => count === 1 ? "1 tâche bloquée" : `${count} tâches bloquées`,
  projectsOverdueTasks: (count: number) => count === 1 ? "1 tâche en retard" : `${count} tâches en retard`,
  projectsWorkload: "Charge de travail",
  projectsWorkloadEmpty: "Aucun travail ouvert n’est encore attribué.",
  projectsOpenTasks: (count: number) => count === 1 ? "1 tâche ouverte" : `${count} tâches ouvertes`,
  projectsBudgetUsed: "Budget consommé",
  projectsBudgetHours: "Budget (heures)",
  projectsBudgetAmount: "Budget (montant)",
  projectsBudgetHint: "Indicatif. Rien n’empêche de saisir une heure au-delà.",
  projectsBudgetHoursInvalid: "Écrivez le budget en nombre entier d’heures.",
  projectsBudgetAmountInvalid:
    "Écrivez le budget sous forme de montant, par exemple 7600,00.",
  projectsLastWorked: "Dernier travail",
  projectsNeverWorked: "Jamais",
  projectsStartsOn: "Commence le",
  projectsMakeClientWork: "Passer en travail client",
  projectsStartTimerOn: (project: string) =>
    `Démarrer le chronomètre sur ${project}`,
  projectsStartTimer: "Démarrer le chronomètre",
  projectsEmptyTitle: "Aucun projet pour l’instant",
  projectsEmptyBody:
    "Créez un projet pour un client ou pour votre entreprise, puis commencez à suivre le temps.",

  // Le formulaire du projet.
  projectsClientSubtitle:
    "Pour qui ce projet est réalisé, et ce que vaut une heure passée dessus.",
  projectsPersonalBoard:
    "Ceci est un tableau personnel. Seul un projet d’équipe peut être du travail client — ses heures sont validées par quelqu’un d’autre et facturées à un client.",
  projectsDetach: "Passer en interne",
  projectsDetachTitle: "Passer ce projet en travail interne ?",
  projectsDetachBody:
    "Les heures restent exactement telles quelles. Ce qui disparaît, c’est le fait qu’elles soient facturables à un client — et les heures déjà portées sur une facture y restent.",

  // La grille de la semaine.
  projectsPreviousWeek: "Précédente",
  projectsNextWeek: "Suivante",
  projectsThisWeek: "Cette semaine",
  projectsWeekOf: (from: string, to: string) => `${from} – ${to}`,
  projectsBillableOf: (hours: string) => `dont ${hours} facturables`,
  projectsWeek: "Semaine",
  projectsDay: "Jour",
  projectsDuration: "Durée",
  projectsDurationHint:
    "90, 1:30 et 1,5 signifient tous une heure et demie. 2h signifie deux heures.",
  projectsDurationInvalid:
    "Écrivez une durée comme 90, 1:30, 1,5 ou 2h — une journée au maximum.",
  projectsTotal: "Total",
  projectsAddRow: "Ajouter une ligne de projet…",
  projectsBillable: "Facturable au client",
  projectsNotBillable: "non facturable",
  projectsNote: "Note",
  projectsNoNote: "Aucune note",
  projectsNoteHint:
    "Ce que vous faisiez. Personne en dehors de cet espace de travail ne la lit.",
  projectsProposedEntry: "proposée",
  projectsBilledEntry: "sur une facture",
  projectsCellLabel: (project: string, day: string, duration: string) =>
    `${project}, ${day} : ${duration}`,
  projectsDeleteEntry: "Supprimer",
  projectsDeleteEntryTitle: "Supprimer ces heures ?",
  projectsDeleteEntryBody:
    "La saisie disparaît définitivement. Sa semaine doit être ouverte pour cela.",
  projectsWeekEmptyTitle: "Rien de saisi cette semaine",
  projectsWeekEmptyBody:
    "Ajoutez votre première saisie de temps. Choisissez un projet, indiquez la durée et une note, puis retrouvez-la dans ce récapitulatif hebdomadaire.",
  projectsWeekTitle: "Feuille de temps hebdomadaire",
  projectsWeekPurpose: "Saisissez votre travail, vérifiez la semaine, puis soumettez-la pour approbation.",
  projectsAddTime: "Ajouter du temps",
  projectsChooseTimeProject: "Sur quoi avez-vous travaillé ?",
  projectsChooseTimeProjectHint: "Choisissez un projet pour ajouter une saisie de temps cette semaine.",
  projectsBillableOfWeek: (duration: string) => `dont ${duration} facturables`,
  projectsProposedInWeek: (duration: string) =>
    `${duration} proposées, pas encore acceptées`,
  // Décider d’une proposition (B3.10b). C’est l’acceptation qui en fait une
  // heure — la formulation le dit, ce que « OK » ne ferait pas.
  projectsAcceptEntry: "Accepter",
  projectsRejectEntry: "Écarter",
  projectsAcceptEntryLabel: (project: string, duration: string) =>
    `Accepter les ${duration} proposées sur ${project}`,
  projectsRejectEntryLabel: (project: string, duration: string) =>
    `Écarter les ${duration} proposées sur ${project}`,
  projectsSuggestionsWaiting: (count: number) =>
    count === 1
      ? "1 proposition vous attend cette semaine."
      : `${count} propositions vous attendent cette semaine.`,
  projectsSubmitWeek: "Soumettre la semaine",
  projectsWithdrawWeek: "Reprendre",
  projectsRejectedBecause: (note: string) => `Renvoyée : ${note}`,

  // Le planning — des jalons sur un axe de dates, au-dessus du tableau qui
  // existe déjà. « Atteint » est délibérément un mot de personne et non
  // « terminé » : un jalon est atteint quand quelqu’un dit que le livrable a
  // été accepté, jamais quand la dernière tâche en dessous a été fermée.
  projectsPlanLoadFailed: "Le planning n’a pas pu être chargé.",
  projectsMilestoneAdd: "Ajouter un jalon",
  projectsMilestoneNew: "Nouveau jalon",
  projectsMilestoneName: "Jalon",
  projectsMilestoneNameHint:
    "Ce à quoi la date correspond — « Design validé », « Bêta chez le pilote ».",
  projectsMilestoneDue: "Date",
  projectsMilestoneDueHint:
    "Le jour prévu. La repousser est ordinaire ; rien n’en est bloqué.",
  projectsMilestoneReach: "Marquer atteint",
  projectsMilestoneReopen: "Pas encore atteint",
  projectsMilestoneReached: "Atteint",
  projectsMilestoneLate: "En retard",
  projectsMilestoneNoTasks: "Aucune tâche en dessous pour l’instant",
  projectsMilestoneTasksClosed: (done: number, total: number) =>
    `${done} tâches fermées sur ${total}`,
  projectsMilestoneDelete: "Supprimer",
  projectsMilestoneDeleteTitle: "Supprimer ce jalon ?",
  projectsMilestoneDeleteBody:
    "La date disparaît ; les tâches en dessous restent exactement où elles sont sur le tableau.",
  projectsPlanUnplaced: "Hors planning",
  projectsPlanPlace: "Placer sous…",
  projectsPlanPlaceTask: (task: string) => `Placer ${task} sous un jalon`,
  projectsPlanRemove: "Retirer",
  projectsPlanEmptyTitle: "Aucun planning pour l’instant",
  projectsPlanEmptyBody:
    "Un jalon est une date nommée sur ce projet — les dates dont un client vous parle. Ajoutez la première, puis placez les tâches du tableau en dessous.",

  // Les modèles : un tableau marqué réutilisable, et la copie qu’on en tire.
  projectsTimelineAllEmptyTitle: "Aucun jalon dans vos projets",
  projectsTimelineAllEmptyBody:
    "Choisissez un projet ci-dessus pour ajouter son premier jalon, ou gardez Tous les projets pour la chronologie du portefeuille.",
  projectsTemplateNew: "Nouveau depuis un modèle",
  projectsTemplateNewTitle: "Partir d’un modèle",
  projectsTemplateNewSubtitle: "La forme du travail, sur de nouvelles dates",
  projectsTemplateCreate: "Créer le projet",
  projectsTemplateWhich: "Modèle",
  projectsTemplateWhichHint:
    "Les cartes, leurs colonnes, les listes de contrôle et les étiquettes suivent — pas les responsables, les commentaires, les heures ni les cartes terminées.",
  projectsTemplateOption: (name: string, tasks: number, milestones: number) =>
    `${name} — ${tasks} ${tasks === 1 ? "carte" : "cartes"}, ${milestones} ${
      milestones === 1 ? "jalon" : "jalons"
    }`,
  projectsTemplateName: "Nom du nouveau projet",
  projectsTemplateNameHint: "Le nom qu’il portera sur le tableau.",
  projectsTemplateStarts: "Commence le",
  projectsTemplateStartsHint:
    "Le premier jalon du modèle tombe à cette date ; toutes les autres gardent leur écart.",
  projectsTemplateCustomerHint:
    "Un modèle est une forme, pas un client. Laissez vide pour du travail interne ; le taux et le budget suivent dans les deux cas.",
  projectsTemplateNoCustomer: "Travail interne",
  projectsTemplateNoPlan:
    "Ce modèle n’a aucun jalon : ses dates sont donc copiées telles quelles.",
  projectsTemplateMarkOn: (project: string) => `Faire de ${project} un modèle`,
  projectsTemplateUnmarkOn: (project: string) =>
    `${project} est un modèle — retirer la marque`,
  projectsTemplateEmptyTitle: "Aucun modèle pour l’instant",
  projectsTemplateChooseProject: "Choisir un projet",
  projectsTemplateEmptyBody:
    "Ouvrez un projet que vous mèneriez de la même façon une deuxième fois et appuyez sur l’étoile à côté. Il reste un tableau ordinaire — il peut simplement être copié.",
  projectsTemplateFailed: "Cela n’a pas pu être fait.",
  projectsTemplatesLoadFailed: "Les modèles n’ont pas pu être chargés.",

  // Où en est une semaine. Le mot du serveur, jamais redéduit dans le
  // navigateur.
  projectsWeekOpen: "Ouverte",
  projectsWeekSubmitted: "Soumise",
  projectsWeekApproved: "Validée",
  projectsWeekRejected: "Renvoyée",

  // La boîte des validations — le seul écran d’ici qui nomme une personne.
  projectsPerson: "Personne",
  projectsSubmittedAt: "Remise le",
  projectsApprove: "Valider",
  projectsReject: "Renvoyer",
  projectsRejectTitle: "Renvoyer cette semaine ?",
  projectsRejectBody: (person: string) =>
    `${person} lira ce que vous écrivez ici.`,
  projectsRejectPlaceholder: "Ce qui est à corriger",
  projectsApprovalsEmptyTitle: "Rien à valider",
  projectsApprovalsEmptyBody:
    "Les semaines remises par vos collègues arrivent ici, les plus anciennes d’abord.",

  // Le rapport de rentabilité — les heures multipliées par les taux, face à un
  // budget. Le mot est « valeur » et jamais « marge » : c’est le côté recettes,
  // et ce qu’une heure nous coûte demande une comptabilité et un dossier
  // salarié qui n’existent ni l’une ni l’autre.
  projectsReportTitle: "Rentabilité",
  projectsReportFrom: "Du",
  projectsReportTo: "Au",
  projectsReportShow: "Afficher",
  projectsReportThisQuarter: "Ce trimestre",
  projectsReportLastQuarter: "Trimestre précédent",
  projectsReportDownloadCsv: "Télécharger le CSV",
  projectsReportDownloadFailed: "Le rapport n’a pas pu être téléchargé.",
  projectsReportBasis: (from: string, to: string) =>
    `Heures travaillées entre le ${from} et le ${to}.`,
  projectsReportBudgetBasis: (to: string) =>
    `Les budgets comptent tout jusqu’au ${to}, et pas seulement cette période.`,
  projectsReportColValue: "Valeur",
  projectsReportColInvoiced: "Facturé",
  projectsReportColToInvoice: "À facturer",
  projectsReportColToDate: "Heures à ce jour",
  projectsReportColBudget: "Budget consommé",
  projectsReportTotals: "Tous les projets",
  projectsReportUnrated: (duration: string) => `${duration} non tarifées`,
  projectsReportUnratedHint:
    "Des heures facturables sans taux. Elles sont comptées ici et valorisées nulle part — tarifez le projet, puis saisissez-les.",
  projectsReportNoValue: "Aucune valeur pour l’instant",
  projectsReportBudgetLeft: (amount: string) => `${amount} restants`,
  projectsReportBudgetOver: (amount: string) => `${amount} de dépassement`,
  projectsReportNoBudget: "Aucun budget défini",
  projectsReportEmptyTitle: "Aucun projet client pour l’instant",
  projectsReportEmptyBody:
    "La rentabilité, ce sont des heures face à un taux et à un budget : elle commence donc par un projet client. Donnez un client et un taux à un projet, et ceci se remplit.",

  // Le chronomètre en marche, dans la barre latérale.
  projectsTimerRunning: "Chronomètre en marche",
  projectsStopTimer: "Arrêter le chronomètre",
  projectsStop: "Arrêter",
  mailAttachmentErrorDetail: (reason: string) =>
    `Ce fichier n’a pas été joint. Essayez de l’ajouter à nouveau. Serveur : ${reason}`,
  mailDraftCreateErrorDetail: (reason: string) =>
    `Votre message n’a pas été envoyé, car son brouillon n’a pas pu être créé. La fenêtre de rédaction reste ouverte ; réessayez d’envoyer. Serveur : ${reason}`,
  mailSubmitErrorDetail: (reason: string) =>
    `Votre message n’a pas été envoyé. Il reste dans les Brouillons afin que vous puissiez le rouvrir et réessayer. Serveur : ${reason}`,
  mailScheduleErrorDetail: (reason: string) =>
    `Votre message n’a pas été programmé. Il reste dans les Brouillons afin que vous puissiez le rouvrir et réessayer. Serveur : ${reason}`,
  driveLoading: "Chargement de vos fichiers…",
  driveLocations: "Emplacements du Drive",
  driveFolderLoading: (name: string) => `Chargement de ${name}…`,
  driveFolderLoadFailed: (reason: string) =>
    `Ce dossier ne s’est pas chargé. Serveur : ${reason}`,
  driveSpacesLoadFailed: (reason: string) =>
    `Vos espaces ne se sont pas chargés. Réessayez. Serveur : ${reason}`,
  driveRetry: "Réessayer",
  driveUnknownError: "Le serveur n’a fourni aucune raison.",
  driveLoadFailedTitle: "Vos fichiers ne se sont pas chargés",
  driveLoadFailed: (reason: string) => `Réessayez. Serveur : ${reason}`,
  driveActionFailed: (action: string, reason: string) =>
    `${action} ne s’est pas terminé. Réessayez. Serveur : ${reason}`,
  driveMovedToTrash: (name: string) => `${name} a été placé dans la corbeille.`,
  driveRestoredFromTrash: (name: string) => `${name} a été restauré.`,
  driveUndo: "Annuler",
  driveSelected: (count: number) =>
    count === 1 ? "1 élément sélectionné" : `${count} éléments sélectionnés`,
  driveSelectItem: (name: string) => `Sélectionner ${name}`,
  driveSelectAll: "Sélectionner tous les éléments visibles",
  driveClearSelection: "Effacer la sélection",
  driveSelectionActions: "Actions sur les éléments sélectionnés",
  driveItemsMovedToTrash: (count: number) =>
    `${count} éléments ont été placés dans la corbeille.`,
  driveItemsRestored: (count: number) => `${count} éléments ont été restaurés.`,
  drivePurgeManyConfirm: (count: number) =>
    `Supprimer définitivement ${count} éléments ? Cette action est irréversible.`,
  driveVersionsLoadFailed: (reason: string) =>
    `L’historique des versions ne s’est pas chargé. Réessayez. Serveur : ${reason}`,
  driveMembersLoadFailed: (reason: string) =>
    `Les membres ne se sont pas chargés. Réessayez. Serveur : ${reason}`,
  baseCalendarPreviousMonth: "Mois précédent",
  baseCalendarNextMonth: "Mois suivant",
  baseCalendarAddOnDate: (date: string) =>
    `Ajouter un enregistrement le ${date}`,
  baseLoading: "Chargement de votre base…",
  baseBoardEmptyTitle: "Regrouper les enregistrements dans un tableau",
  baseCalendarEmptyTitle: "Placer les enregistrements dans un calendrier",
  baseBoardEmptyBody:
    "Les tableaux regroupent les enregistrements selon un champ de sélection. Ajoutez un champ Statut prêt à l’emploi pour continuer.",
  baseCalendarEmptyBody:
    "Les calendriers placent les enregistrements selon un champ Date. Ajoutez-en un pour continuer.",
  baseAddStatusField: "Ajouter le champ Statut",
  baseAddDateField: "Ajouter le champ Date",
  baseStatusField: "Statut",
  baseDateField: "Date",
  baseStatusTodo: "À faire",
  baseStatusInProgress: "En cours",
  baseStatusDone: "Terminé",
  baseLoadFailedTitle: "Cette base ne s’est pas chargée",
  baseEmptyTitle: "Commencez par votre première table",
  baseEmptyBody:
    "Les tables regroupent les enregistrements associés. Créez-en une pour ajouter des champs et des enregistrements.",
  baseDefaultTableName: (number: number) => `Table ${number}`,
  baseView: "Vue",
  baseSaveChanges: "Enregistrer les modifications",
  officeLoading: "Ouverture de l’éditeur Office…",
  officeDiscoveryMissing:
    "L’éditeur Office n’a pas publié d’adresse d’éditeur.",
  officeLoadFailed: (reason: string) => `Réessayez. Serveur : ${reason}`,
  sheetLoading: "Chargement de votre feuille…",
  sheetLoadFailedTitle: "Cette feuille ne s’est pas chargée",
  docLoading: "Chargement de votre document…",
  docLoadFailedTitle: "Ce document ne s’est pas chargé",
  docSaveFailed: (reason: string) =>
    `Vos dernières modifications ne sont pas encore enregistrées. Choisissez Réessayer pour les enregistrer. Serveur : ${reason}`,
  sheetSaveFailed: (reason: string) =>
    `Vos dernières modifications ne sont pas encore enregistrées. Nous continuerons d’essayer. Serveur : ${reason}`,
  sitesSubmissions: "Messages reçus",
  sitesSubmissionsLoadFailed:
    "Les messages de vos formulaires n’ont pas pu être chargés.",
  sitesSubmissionSaveFailed: "Ce message n’a pas pu être mis à jour.",
  sitesNoSubmissionsTitle: "Aucun message pour le moment",
  sitesNoSubmissionsBody:
    "Ajoutez un formulaire de contact à une page. Les messages des visiteurs apparaîtront ici.",
  sitesOpenPages: "Ouvrir les pages",
  sitesSubmissionList: "Messages des visiteurs",
  sitesSubmissionDetail: "Message sélectionné",
  sitesHandled: "Traité",
  sitesNeedsReply: "À traiter",
  sitesMarkHandled: "Marquer comme traité",
  sitesReopenSubmission: "Rouvrir",
  sitesForm: "Formulaire",
  sitesReceived: "Reçu",
  sitesExportSubmissions: "Exporter en CSV",
  sitesExportingSubmissions: "Préparation de l’export…",
  sitesSubmissionsExportFailed:
    "Vos messages n’ont pas pu être exportés. Réessayez.",
  sitesAssistant: "Assistant",
  sitesAssistantTitle: "Assistant du site",
  sitesAssistantLoadFailed:
    "Les réglages de l’assistant n’ont pas pu être chargés. Réessayez.",
  sitesAssistantSwitchTitle: "L’assistant et son budget",
  sitesAssistantSwitchHint:
    "Un assistant de conversation sur votre site publié qui répond aux questions des visiteurs à partir de vos pages publiées — et cite toujours la page d’où vient une réponse.",
  sitesAssistantEnable: "Répondre aux questions des visiteurs sur le site publié",
  sitesAssistantBudgetLabel: "Budget mensuel (€)",
  sitesAssistantBudgetHint: (defaultBudget: string) =>
    `Les réponses coûtent de l’argent. Quand les réponses d’un mois atteignent ce budget, l’assistant se met en pause et les visiteurs sont orientés vers votre formulaire de contact — vous en serez averti. Sans réglage, le budget est de ${defaultBudget}.`,
  sitesAssistantBudgetNotANumber: "Saisissez le budget mensuel en euros.",
  sitesAssistantSpent: (spent: string, budget: string) =>
    `${spent} dépensés sur ${budget} ce mois-ci.`,
  sitesAssistantCeilingHit:
    "Le budget de ce mois est épuisé : l’assistant est en pause et les visiteurs sont orientés vers votre formulaire de contact. Augmenter le budget le rouvre immédiatement.",
  sitesAssistantSave: "Enregistrer",
  sitesAssistantSaved: "Enregistré.",
  sitesAssistantSaveFailed:
    "Les réglages de l’assistant n’ont pas pu être enregistrés. Réessayez.",
  sitesAssistantReadsTitle: "Ce que l’assistant lit",
  sitesAssistantReadsRule:
    "Tout ce que l’assistant peut lire, n’importe qui sur Internet peut le lire — il s’en sert pour répondre à des inconnus.",
  sitesAssistantReadsPublishedSite: "Votre site publié — chaque page en ligne",
  sitesAssistantReadsPublishedPosts: "Vos articles de blog publiés",
  sitesAssistantAlwaysRead: "toujours lu",
  sitesAssistantNoKnowledge:
    "Aucun document publié vers l’assistant pour l’instant. Il répond uniquement à partir de votre site publié.",
  sitesAssistantAddedOn: (date: string) => `publié le ${date}`,
  sitesAssistantTrashed: "dans la corbeille de Drive — n’est plus lu",
  sitesAssistantWithdraw: (title: string) => `Retirer ${title}`,
  sitesAssistantWithdrawFailed:
    "Le document n’a pas pu être retiré de l’assistant. Réessayez.",
  sitesAssistantInternetWarning:
    "N’importe qui sur Internet pourra lire ceci.",
  sitesAssistantPublishDocument: "Publier un document vers l’assistant…",
  sitesAssistantPublishFailed:
    "Le document n’a pas pu être publié vers l’assistant. Réessayez.",
  sitesAssistantPickerTitle: "Publier un document vers l’assistant",
  sitesAssistantPickerSubtitle:
    "Choisissez un document lisible — l’assistant s’en servira pour répondre aux visiteurs.",
  sitesAssistantPickerConfirm: "Publier vers l’assistant",
  sitesAssistantPickerBack: "Revenir au dossier parent",
  sitesAssistantPickerSearch: "Rechercher dans ce dossier",
  sitesAssistantPickerEmpty: "Rien dans ce dossier.",
  // Sites — le journal des actions de l'assistant (ADR 0040, S3.03e).
  sitesAssistantDidTitle: "Ce que l'assistant a fait",
  sitesAssistantDidHint:
    "Chaque action de l'assistant en votre nom, avec le fait utilisé et la page d'où ce fait provient. Ce que les visiteurs tapent n'est jamais conservé.",
  sitesAssistantDidEmpty:
    "Rien pour l'instant. Dès que l'assistant répond à une question, propose des créneaux, réserve un rendez-vous ou enregistre un prospect, chaque action apparaît ici.",
  sitesAssistantDidLoadFailed:
    "Les actions de l'assistant n'ont pas pu être chargées. Réessayez.",
  sitesAssistantDidAnswered: "A répondu à une question",
  sitesAssistantDidAnsweredUsing: (pages: string) =>
    `A répondu à une question à partir de ${pages}`,
  sitesAssistantDidRefused:
    "A décliné une question à laquelle vos pages publiées ne permettaient pas de répondre",
  sitesAssistantDidBookingOffered: (service: string) =>
    `A proposé des créneaux pour « ${service} »`,
  sitesAssistantDidBooked: (service: string, when: string) =>
    `A réservé « ${service} » pour ${when} — le rendez-vous est dans votre calendrier`,
  sitesAssistantDidLeadOffered:
    "A proposé le formulaire de contact dans la conversation",
  sitesAssistantDidLeadSaved:
    "A enregistré un nouveau prospect sur votre tableau CRM",
  sitesAssistantDidLeadKnown:
    "A indiqué à un contact qui revenait que vous le connaissez déjà — aucun doublon n'a été créé",
  sitesAssistantDidTicketsOffered: (event: string) =>
    `A proposé des billets pour « ${event} » au prix de votre liste de prix`,
  // Sites — l'écran d'apparence de l'assistant (ADR 0040 §5, S3.02g).
  sitesAssistantLookTitle: "Son apparence et sa voix",
  sitesAssistantLookHint:
    "Le widget porte déjà le thème, le logo et la langue de votre site. Vous choisissez ici ses mots et quelques réglages encadrés — la couleur reste dans la palette de votre site.",
  sitesAssistantBotNameLabel: "Nom de l'assistant",
  sitesAssistantBotNameHint:
    "Souvent volontairement différent du nom de l'entreprise — « Demandez à Marie » fonctionne mieux que « Discutez avec nous ».",
  sitesAssistantAvatarLabel: "Avatar",
  sitesAssistantAvatarHint:
    "Une petite photo affichée dans l'en-tête du widget. Un visage fonctionne mieux qu'un logo.",
  sitesAssistantWelcomeLabel: "Message d'accueil",
  sitesAssistantWelcomeDefaultNote:
    "Voici le texte par défaut, dans la langue de votre site — gardez-le ou faites-le vôtre.",
  sitesAssistantQuestionsLegend: "Questions suggérées",
  sitesAssistantQuestionsHint:
    "Jusqu'à trois questions à toucher, proposées jusqu'à ce que le visiteur pose la sienne.",
  sitesAssistantQuestionLabel: (n: number) => `Question suggérée ${n}`,
  sitesAssistantSuggestFromSite: "Suggérer depuis votre site",
  sitesAssistantSuggestedApplied:
    "Rédigées à partir des pages de votre site — modifiez-les librement.",
  sitesAssistantSuggestedNone:
    "Rien à proposer pour l'instant. Une FAQ, des tarifs, une réservation ou un formulaire de contact sur vos pages donneront matière à suggestion.",
  sitesAssistantSuggestFailed:
    "Vos pages n'ont pas pu être lues pour les suggestions. Réessayez.",
  sitesAssistantSuggestedPricing: "Quels sont vos tarifs ?",
  sitesAssistantSuggestedBooking: "Puis-je prendre rendez-vous ?",
  sitesAssistantSuggestedCatalog: "Que proposez-vous ?",
  sitesAssistantSuggestedContact: "Comment puis-je vous joindre ?",
  sitesAssistantAppearanceSave: "Enregistrer l'apparence",
  sitesAssistantToneLegend: "Ton",
  sitesAssistantToneFormal: "Formel",
  sitesAssistantToneNeutral: "Neutre",
  sitesAssistantToneWarm: "Chaleureux",
  sitesAssistantToneNoteLabel: "Note de voix",
  sitesAssistantToneNoteHint:
    "La manière dont votre entreprise s'exprime — mots simples, pas de jargon, par exemple. Style uniquement : cela ne change jamais ce que l'assistant a le droit de dire ou de promettre.",
  sitesAssistantCornerLegend: "Coin du lanceur",
  sitesAssistantCornerRight: "En bas à droite",
  sitesAssistantCornerLeft: "En bas à gauche",
  sitesAssistantIconLegend: "Icône du lanceur",
  sitesAssistantIconChat: "Bulle de dialogue",
  sitesAssistantIconQuestion: "Point d'interrogation",
  sitesAssistantIconSparkle: "Étincelle",
  sitesAssistantAccentLegend: "Couleur",
  sitesAssistantAccentHint:
    "Un choix parmi les rôles de la palette de votre site — chaque option garde un contraste lisible.",
  sitesAssistantAccentPrimary: "Couleur de marque",
  sitesAssistantAccentText: "Encre",
  sitesAssistantAccentSurface: "Discret",
  sitesAssistantAutoOpenLabel: "S'ouvre de lui-même au chargement de la page",
  sitesAssistantAutoOpenHint:
    "Désactivé par défaut — une fenêtre non sollicitée est ce que tout le monde déteste. Activé, il s'ouvre sans voler le clavier.",
  sitesAssistantOfflineLabel: "Message d'indisponibilité",
  sitesAssistantOfflineHint:
    "Affiché quand l'assistant ne peut pas répondre — budget mensuel épuisé, ou aucune IA configurée.",
  sitesAssistantPreviewTitle: "Aperçu",
  sitesAssistantPreviewHint:
    "Le vrai widget, dans le thème de votre site, montré ouvert. Les visiteurs le voient d'abord fermé dans son coin.",
  sitesAssistantPreviewFrameTitle: "Aperçu du widget de l'assistant",
  sitesAssistantPreviewFailed: "L'aperçu n'a pas pu être affiché.",
  sitesAssistantA11yTitle: "Accessibilité",
  sitesAssistantA11yContrast: (ratio: string) =>
    `Le texte sur la couleur choisie mesure ${ratio}:1 — au-dessus du seuil WCAG AA de 4,5:1.`,
  sitesAssistantA11yContrastGuarantee:
    "Chaque choix de couleur est vérifié côté serveur contre votre palette — aucune option ne peut enregistrer une combinaison illisible.",
  sitesAssistantA11yKeyboard:
    "Le widget est une boîte de dialogue étiquetée : utilisable entièrement au clavier, Échap la ferme, et les réponses sont annoncées par les lecteurs d'écran à mesure qu'elles arrivent.",
  sitesAssistantA11yAvatar:
    "L'avatar est décoratif et masqué aux lecteurs d'écran — c'est le nom de l'assistant qu'ils annoncent.",
  sitesAnalytics: "Statistiques",
  sitesAnalyticsLoadFailed:
    "Les statistiques de votre site n’ont pas pu être chargées. Réessayez.",
  sitesAnalyticsLoading: "Chargement des statistiques du site",
  sitesAnalyticsPeriod: "Période des statistiques",
  sitesAnalyticsDays: (days: number) => `${days} jours`,
  sitesAnalyticsSummary: "Résumé du trafic",
  sitesAnalyticsVisits: "Visites",
  sitesAnalyticsVisitors: "Visiteurs quotidiens",
  sitesAnalyticsOverTime: "Visites au fil du temps",
  sitesAnalyticsChartLabel: "Visites quotidiennes du site",
  sitesAnalyticsDayLabel: (date: string, visits: number) =>
    `${date} : ${visits} ${visits === 1 ? "visite" : "visites"}`,
  sitesAnalyticsTopPages: "Pages les plus visitées",
  sitesAnalyticsTopReferrers: "Principales sources",
  sitesAnalyticsDirect: "Accès direct",
  sitesAnalyticsPrivacyTitle: "Sans cookies. Sans bannière.",
  sitesAnalyticsPrivacyBody:
    "Le trafic est compté anonymement par jour. alo ne conserve ni adresse du visiteur, ni profil d’appareil, ni historique de navigation.",
  sitesAnalyticsEmptyTitle: "Aucune visite pour le moment",
  sitesAnalyticsEmptyBody:
    "Ouvrez ou partagez votre site publié. Ses premières visites apparaîtront ici automatiquement.",
  sitesAnalyticsOpenSite: "Ouvrir le site publié",
  sitesAnalyticsPrivacyBeacon:
    "Le temps de lecture et les clics sortants sont signalés par un petit script présent sur vos pages. Il ne porte aucune identité, si bien que deux signalements d’un même navigateur ne peuvent pas être reliés.",
  // Sites — les panneaux de détail regroupés (S2.08b).
  sitesAnalyticsGroupArrival: "Comment on vous a trouvé",
  sitesAnalyticsGroupPages: "Ce qui a été consulté",
  sitesAnalyticsGroupReading: "Comment on vous a lu",
  sitesAnalyticsShowAll: (count: number) => `Tout afficher (${count})`,
  sitesAnalyticsShowTop: (count: number) => `Afficher les ${count} premiers`,
  sitesAnalyticsReferrersNote:
    "Le site depuis lequel un visiteur a suivi un lien. Seul le domaine est conservé, jamais la page.",
  sitesAnalyticsReferrersEmpty:
    "Aucune source pour le moment. Elles apparaissent lorsqu’un autre site pointe vers le vôtre.",
  sitesAnalyticsCampaigns: "Campagnes",
  sitesAnalyticsCampaignsNote:
    "Lues dans le paramètre utm_campaign des liens que vous partagez, pour distinguer une infolettre d’une affiche.",
  sitesAnalyticsCampaignsEmpty:
    "Aucune campagne pour le moment. Ajoutez ?utm_campaign=mailing-printemps à un lien que vous partagez et ses visites seront comptées ici.",
  sitesAnalyticsNoCampaign: "Sans campagne",
  sitesAnalyticsCountries: "Pays",
  sitesAnalyticsCountriesNote:
    "Déterminés par le réseau placé devant votre site, jamais à partir d’une adresse de visiteur conservée.",
  sitesAnalyticsCountriesEmpty:
    "Aucun pays signalé. Votre site est servi sans réseau qui les nomme : ce panneau reste vide, et tous les autres chiffres restent complets.",
  sitesAnalyticsNotReported: "Non communiqué",
  sitesAnalyticsTopPagesNote: "Les pages les plus ouvertes.",
  sitesAnalyticsPagesEmpty: "Aucune page comptée sur cette période.",
  sitesAnalyticsEntryPages: "Pages d’arrivée",
  sitesAnalyticsEntryPagesNote:
    "La page par laquelle la journée d’un visiteur sur votre site a commencé.",
  sitesAnalyticsExitPages: "Dernières pages",
  sitesAnalyticsExitPagesNote:
    "La dernière page vue ce jour-là. C’est là que la lecture s’est terminée, pas nécessairement là où elle a été abandonnée.",
  sitesAnalyticsReadTime: "Temps de lecture",
  sitesAnalyticsReadTimeNote:
    "Combien de temps les pages sont restées à l’écran, pour l’ensemble du site et non page par page. Seuls les navigateurs qui le signalent sont comptés : ces chiffres n’atteignent donc jamais votre total de visites.",
  sitesAnalyticsReadTimeEmpty:
    "Aucun temps de lecture pour le moment. Ils arrivent dès que des visiteurs ouvrent vos pages publiées dans un navigateur qui les signale.",
  sitesAnalyticsReadUnder10s: "Moins de 10 secondes",
  sitesAnalyticsRead10to30s: "10 à 30 secondes",
  sitesAnalyticsRead30to60s: "30 à 60 secondes",
  sitesAnalyticsRead1to3m: "1 à 3 minutes",
  sitesAnalyticsRead3to10m: "3 à 10 minutes",
  sitesAnalyticsReadOver10m: "Plus de 10 minutes",
  sitesAnalyticsOutbound: "Liens sortants",
  sitesAnalyticsOutboundNote:
    "Les domaines vers lesquels les visiteurs sont partis. Au-delà de 200 destinations par jour, les suivantes sont comptées ensemble.",
  sitesAnalyticsOutboundEmpty:
    "Aucun clic sortant pour le moment. Ils sont comptés lorsqu’un visiteur suit un lien vers un autre site.",
  sitesAnalyticsOutboundOther: "Autres domaines",
  sitesAnalyticsDevices: "Appareils",
  sitesAnalyticsDevicesNote:
    "Une catégorie sommaire, tirée de ce que le navigateur dit de lui-même. Rien de plus n’en est conservé.",
  sitesAnalyticsDevicesEmpty: "Aucun appareil compté sur cette période.",
  sitesAnalyticsDevicePhone: "Téléphone",
  sitesAnalyticsDeviceTablet: "Tablette",
  sitesAnalyticsDeviceDesktop: "Ordinateur",
  sitesAnalyticsDeviceBot: "Robots et moteurs",
  sitesAnalyticsDeviceUnknown: "Non reconnu",
  // Sites — la carte d'attention (S2.09b).
  sitesHeatmap: "Carte d’attention",
  sitesBackToAnalytics: "Retour aux statistiques",
  sitesHeatmapLoadFailed:
    "La carte d’attention n’a pas pu être chargée. Réessayez.",
  sitesHeatmapLoading: "Chargement de la carte d’attention",
  sitesHeatmapPage: "Page",
  sitesHeatmapPageOption: (path: string, events: number) =>
    `${path} — ${events} comptés`,
  sitesHeatmapScreens: "Taille d’écran",
  sitesHeatmapScreenTab: (screen: string, events: string) =>
    `${screen} (${events})`,
  sitesHeatmapPrivacyTitle: "Une forme, pas un enregistrement.",
  sitesHeatmapPrivacyBody:
    "Les clics et la profondeur de lecture sont comptés par zone de la page, par jour. Aucun tracé du curseur, aucun rejeu de session, et rien qui puisse relier deux visites à la même personne.",
  sitesHeatmapPrivacyShape:
    "Seuls les navigateurs qui les signalent sont comptés, et au plus vingt clics par affichage de page. Lisez ceci comme l’endroit où l’attention s’est portée — jamais comme le nombre de personnes qui ont fait quelque chose.",
  sitesHeatmapEmptyTitle: "Rien à cartographier pour l’instant",
  sitesHeatmapEmptyBody:
    "Les clics et la profondeur de lecture apparaissent ici dès que des visiteurs ouvrent vos pages publiées. Rien n’est à activer.",
  sitesHeatmapClicks: "Où l’on a cliqué",
  sitesHeatmapClicksNote:
    "La page entière, de haut en bas, et non un seul écran. Un carré plus foncé est une zone davantage cliquée.",
  sitesHeatmapClicksLabel: (path: string, screen: string, clicks: number) =>
    `Carte des ${clicks} clics reçus sur ${path}, sur ${screen}`,
  sitesHeatmapTop: "Haut de la page",
  sitesHeatmapBottom: "Bas de la page",
  sitesHeatmapLegendQuiet: "Moins",
  sitesHeatmapLegendBusy: "Plus",
  sitesHeatmapLeft: "Gauche",
  sitesHeatmapCentre: "Centre",
  sitesHeatmapRight: "Droite",
  sitesHeatmapSpot: (side: string, band: string) => `${side}, ${band}`,
  sitesHeatmapDepthBand: (from: number, to: number) =>
    `${from}–${to} % de la page`,
  sitesHeatmapSpots: "Zones les plus actives",
  sitesHeatmapSpotsNote:
    "La même carte en mots, pour qu’elle se lise sans les couleurs.",
  sitesHeatmapClicksEmpty:
    "Rien n’a été cliqué sur cette page sur cette taille d’écran.",
  sitesHeatmapSpotsEmpty: "Rien à décrire pour l’instant.",
  sitesHeatmapSpotsHeldBack:
    "Gardé jusqu’à ce qu’assez de clics soient comptés pour les décrire.",
  sitesHeatmapDepth: "Jusqu’où l’on a lu",
  sitesHeatmapDepthNote:
    "Combien de lecteurs ont atteint chaque dixième de la page. Seuls les navigateurs qui le signalent sont comptés : ce total n’égale donc jamais vos visites.",
  sitesHeatmapDepthEmpty:
    "Aucune profondeur de lecture comptée ici sur cette taille d’écran.",
  sitesHeatmapTooFewTitle: "Trop peu pour dessiner une carte",
  sitesHeatmapTooFewClicks: (collected: number, needed: number) =>
    `${collected} clics sur ${needed} comptés sur cette taille d’écran. Une carte tirée d’une poignée de clics montre cette poignée, pas vos visiteurs : elle est donc gardée jusqu’à ce qu’il y en ait assez.`,
  sitesHeatmapTooFewDepth: (collected: number, needed: number) =>
    `${collected} signalements de lecture sur ${needed} comptés sur cette taille d’écran. La courbe apparaît dès qu’il y en a assez pour qu’elle ait un sens.`,
  // Sites — ce que le site a rapporté (S2.10c) : de la vue de page à la
  // facture, via la jonction CRM/Facturation construite en S2.10b.
  sitesFunnel: "Résultats",
  sitesFunnelPeriod: "Période",
  sitesFunnelLoading: "Chargement des résultats",
  sitesFunnelLoadFailed:
    "Les résultats n’ont pas pu être chargés. Réessayez.",
  sitesFunnelDeniedTitle: "En dehors de vos accès",
  sitesFunnelDeniedFallback:
    "Cette page lit alo CRM et alo Facturation, qui ne sont pas ouverts pour ce compte.",
  sitesFunnelDeniedWay:
    "Tout le reste de ce site — ses pages, ses demandes et son trafic — reste à vous.",
  sitesFunnelNoSourcesTitle: "Pas encore de formulaire de contact",
  sitesFunnelNoSourcesBody:
    "Ajoutez un formulaire de contact à une page : chaque demande qu’il apporte pourra être suivie de la première vue de page jusqu’à la facture.",
  sitesFunnelChain: "Du visiteur à la facture",
  sitesFunnelStageViews: "Ont vu le formulaire",
  sitesFunnelStageStarts: "Ont commencé à écrire",
  sitesFunnelStageSubmits: "Demandes",
  sitesFunnelStageLeads: "Transmises aux ventes",
  sitesFunnelStageWon: "Gagnées",
  sitesFunnelStageInvoices: "Factures",
  sitesFunnelFromBrowser: "Signalé par le navigateur",
  sitesFunnelFromRecord: "Compté à l’enregistrement",
  sitesFunnelFloorNote:
    "Les deux premières étapes sont signalées par le navigateur du visiteur, et un navigateur qui ne signale rien a tout de même vu la page. À partir de la demande, tout est compté au moment où l’enregistrement a été écrit. Lisez ces chiffres comme un minimum : un taux qui franchit cette limite est le plus bas possible, pas une mesure.",
  sitesFunnelMoney: "L’argent derrière",
  sitesFunnelInvoiceRule:
    "Factures émises pour le client qu’une demande est devenue, après sa transmission.",
  sitesFunnelMoneyEmpty:
    "Aucune opportunité n’a encore été créée depuis ce site.",
  sitesFunnelOpen: "En cours",
  sitesFunnelWon: "Gagné",
  sitesFunnelInvoiced: "Facturé",
  sitesFunnelHidden: "Non affiché",
  sitesFunnelBillingOff:
    "Les montants facturés ne sont pas affichés parce qu’alo Facturation n’est pas ouvert pour ce compte. Ce n’est pas la même chose que « rien n’a été facturé ».",
  sitesFunnelCurrencies:
    "Deux devises font deux lignes et aucun total : une prévision n’a pas de date d’émission à laquelle convertir.",
  sitesFunnelSources: "Par formulaire de contact",
  sitesFunnelColSource: "Formulaire de contact",
  sitesFunnelColDeals: "Opportunités",
  sitesFunnelDealsSummary: (open: number, won: number, lost: number) =>
    `${open} en cours · ${won} gagnées · ${lost} perdues`,
  sitesFunnelSumNote:
    "Une facture atteignable depuis deux formulaires compte une fois pour le site et une fois sous chaque formulaire : ces colonnes sont une lecture par formulaire et ne s’additionnent pas jusqu’aux totaux ci-dessus.",
  sitesFunnelDeletedSource: "Formulaire supprimé",
  sitesFunnelChatSource: "Assistant du site",
  // Sites — transmettre une demande au tableau des ventes (S2.10c).
  sitesHandoffSection: "Ventes",
  sitesHandoffInvite:
    "Transformez cette demande en opportunité sur votre tableau des ventes. Rien de cet écran n’est à retaper.",
  sitesHandoffTitle: "Transmettre cette demande aux ventes",
  sitesHandoffSubtitle:
    "Crée une opportunité sur votre tableau des ventes et la relie à cette demande.",
  sitesHandoffSubmit: "Transmettre aux ventes",
  sitesHandoffFrom: "De",
  sitesHandoffCarried:
    "Le nom, l’adresse et le message accompagnent la transmission — vous ne les retapez jamais.",
  sitesHandoffTitleFor: (who: string) => `Demande du site — ${who}`,
  sitesHandoffBoard: "Tableau",
  sitesHandoffColumn: "Colonne",
  sitesHandoffCardTitle: "Opportunité",
  sitesHandoffValue: "Valeur estimée",
  sitesHandoffValueHint: "Facultatif — ce que vous pensez qu’elle vaut.",
  sitesHandoffCurrency: "Devise",
  sitesHandoffCurrencyHint:
    "Laissez vide pour la devise de votre espace de travail.",
  sitesHandoffLoadingBoards: "Chargement de vos tableaux des ventes…",
  sitesHandoffNoBoards:
    "Il n’y a pas encore de tableau des ventes. Ouvrez alo CRM une fois et votre premier tableau est créé pour vous.",
  sitesHandoffCrmDenied: "alo CRM n’est pas ouvert pour ce compte.",
  sitesHandoffBoardsFailed:
    "Vos tableaux des ventes n’ont pas pu être chargés. Réessayez.",
  sitesHandoffFailed:
    "Cette demande n’a pas pu être transmise. Réessayez.",
  sitesInSales: "Aux ventes",
  sitesLeadsLoadFailed:
    "Les liens vers les ventes n’ont pas pu être chargés pour cette boîte.",
  sitesLeadStanding: (state: string, value: string) => `${state} · ${value}`,
  sitesLeadOpen: "En cours",
  sitesLeadWon: "Gagnée",
  sitesLeadLost: "Perdue",
  sitesUnlinkLead: "Détacher",
  sitesUnlinkLeadFailed:
    "Le lien n’a pas pu être retiré. L’opportunité elle-même n’est pas touchée. Réessayez.",
  // Sites — l'historique des versions publiées (S2.04b).
  sitesHistory: "Historique des versions",
  sitesHistorySubtitle:
    "Chaque version de ce site que vous avez publiée. Consultez-en une, et remettez-la en ligne en un clic.",
  sitesHistoryLoadFailed:
    "L’historique des versions n’a pas pu être chargé.",
  sitesHistoryVersions: "Versions publiées",
  sitesHistoryLiveNow: "En ligne",
  sitesHistoryVersionOf: (date: string) => `Version du ${date}`,
  sitesHistoryPagesCount: (pages: number) =>
    `${pages} ${pages === 1 ? "page" : "pages"}`,
  sitesHistoryLanguages: (languages: string) => `Langues : ${languages}`,
  sitesHistoryRestoredCopy: (date: string) =>
    `Une copie de la version du ${date}`,
  sitesHistoryRestore: "Remettre cette version en ligne",
  sitesHistoryRestoring: "Remise en ligne…",
  sitesHistoryRestoreFailed:
    "Cette version n’a pas pu être remise en ligne.",
  sitesHistoryRestored: (date: string) =>
    `La version du ${date} est de nouveau en ligne.`,
  sitesHistoryUndo: "Annuler",
  sitesHistoryUndone: (date: string) =>
    `Retour à la version du ${date}. Rien n’est perdu : toutes les versions sont toujours là.`,
  sitesHistoryPage: "Page",
  sitesHistoryPreviewLoadFailed: "Cette version n’a pas pu être affichée.",
  sitesHistoryPreviewLoading: "Chargement de cette version",
  sitesHistoryPreviewTitle: "Aperçu de la version publiée",
  sitesHistoryDraftSafe:
    "Votre travail en cours reste intact : remettre une version en ligne ne change jamais ce que vous êtes en train de modifier.",
  sitesHistoryIfRestored: "Si vous remettez cette version en ligne",
  sitesHistoryIdentical: "C’est exactement ce qui est en ligne actuellement.",
  sitesHistoryThemeChange: "L’apparence du site changerait.",
  sitesHistoryLanguagesBack: (languages: string) =>
    `Ces langues reviendraient : ${languages}`,
  sitesHistoryLanguagesGone: (languages: string) =>
    `Ces langues disparaîtraient : ${languages}`,
  sitesHistoryPageBack: (page: string) => `${page} reviendrait`,
  sitesHistoryPageGone: (page: string) => `${page} disparaîtrait`,
  sitesHistoryPageChanged: (page: string) => `${page} changerait`,
  sitesHistoryUnchangedPages: (pages: number) =>
    `${pages} ${pages === 1 ? "page reste identique" : "pages restent identiques"}`,
  sitesHistoryEmptyTitle: "Rien n’est encore publié",
  sitesHistoryEmptyBody:
    "Publiez ce site une fois, et chaque version publiée restera ici — à consulter, et à remettre en ligne.",

  // Sites — publication à un moment choisi (S2.05b).
  sitesScheduleTitle: "Publier à un moment choisi",
  sitesScheduleHint:
    "Choisissez une date et une heure : ce site se mettra en ligne tout seul. Vous n’avez pas besoin d’être là.",
  sitesScheduleLoading: "Vérification de ce qui est programmé",
  sitesScheduleLoadFailed:
    "La publication programmée n’a pas pu être chargée.",
  sitesScheduleOpen: "Programmer la publication",
  sitesScheduleChange: "Changer le moment",
  sitesScheduleWhen: "Date et heure",
  sitesScheduleGoesLive: (moment: string) => `En ligne le ${moment}.`,
  sitesScheduleTimeZone: (zone: string) =>
    `C’est votre heure locale (${zone}), pas celle du serveur.`,
  sitesScheduleSave: "Programmer la publication",
  sitesScheduleMove: "Déplacer à ce moment",
  sitesScheduleSaving: "Enregistrement…",
  sitesScheduleMissingMoment: "Choisissez d’abord une date et une heure.",
  sitesScheduleSaveFailed: "Ce site n’a pas pu être programmé.",
  sitesSchedulePending: (moment: string) =>
    `Ce site se publiera le ${moment}. Tout ce que vous enregistrez d’ici là partira en ligne avec lui.`,
  sitesSchedulePublishingNow: "Ce site est en cours de publication.",
  sitesScheduleCancel: "Annuler",
  sitesScheduleCancelling: "Annulation…",
  sitesScheduleCancelFailed:
    "La publication programmée n’a pas pu être annulée.",
  sitesScheduleCancelled: (moment: string) =>
    `Annulé. Ce site ne sera pas publié le ${moment}, et rien de ce qui est en ligne n’a changé.`,
  sitesScheduleDone: (moment: string) =>
    `Ce site s’est publié tout seul le ${moment}.`,
  sitesScheduleFailed: (moment: string, reason: string) =>
    `Ce site n’a pas pu être publié le ${moment} : ${reason}`,

  // Sites — une page derrière un mot de passe (S2.06b).
  sitesPagePasswordTitle: "Qui peut ouvrir cette page",
  sitesPagePasswordLoading: "Vérification de qui peut ouvrir cette page",
  sitesPagePasswordLoadFailed:
    "Impossible de vérifier si cette page demande un mot de passe.",
  sitesPagePasswordUnknown:
    "On ne sait pas pour l’instant si cette page demande un mot de passe aux visiteurs.",
  sitesPagePasswordPublic:
    "N’importe qui sur Internet peut ouvrir cette page.",
  sitesPagePasswordPublicHint:
    "Donnez-lui un mot de passe et seules les personnes à qui vous le confiez pourront la lire. Le reste de ce site reste public.",
  sitesPagePasswordProtected: (moment: string) =>
    `Seules les personnes qui ont le mot de passe peuvent ouvrir cette page — défini le ${moment}.`,
  sitesPagePasswordProtectedUndated:
    "Seules les personnes qui ont le mot de passe peuvent ouvrir cette page.",
  sitesPagePasswordProtectedHint:
    "Tous les autres tombent sur un écran de déverrouillage qui ne montre rien de la page, pas même son titre. Le mot de passe l’ouvre pour le reste de la journée.",
  sitesPagePasswordEveryLanguage:
    "Cela vaut pour la page dans toutes les langues où elle est publiée.",
  sitesPagePasswordProtect: "Protéger cette page",
  sitesPagePasswordChange: "Changer le mot de passe",
  sitesPagePasswordField: "Mot de passe",
  sitesPagePasswordFieldHint:
    "Personne ne pourra vous le relire ensuite, nous compris : un mot de passe oublié se remplace, il ne se récupère pas.",
  sitesPagePasswordEffective:
    "L’effet est immédiat. Vous n’avez pas besoin de publier le site à nouveau.",
  sitesPagePasswordShow: "Afficher",
  sitesPagePasswordHide: "Masquer",
  sitesPagePasswordSaving: "Enregistrement…",
  sitesPagePasswordMissing: "Saisissez d’abord un mot de passe.",
  sitesPagePasswordSaveFailed: "Cette page n’a pas pu être protégée.",
  sitesPagePasswordSaved:
    "Enregistré. Les visiteurs ont désormais besoin de ce mot de passe, et quiconque avait ouvert la page avec l’ancien devra le saisir à nouveau.",
  sitesPagePasswordRemove: "Retirer le mot de passe",
  sitesPagePasswordRemoveConfirm: "Oui, la rendre publique",
  sitesPagePasswordRemoveFailed: "Le mot de passe n’a pas pu être retiré.",
  sitesPagePasswordRemoved:
    "Le mot de passe est retiré. N’importe qui sur Internet peut de nouveau ouvrir cette page.",
  sitesPagePasswordPreviewNote:
    "Le mot de passe est demandé aux visiteurs avant tout. Cet aperçu montre la page telle que la voit quelqu’un qui l’a.",
  sitesPagePasswordBadge: "Mot de passe",
  sitesPosts: "Articles du blog",
  sitesBackToWebsite: "Site web",
  sitesPostsLoadFailed: "Les articles de votre blog n’ont pas pu être chargés.",
  sitesLoadingPosts: "Chargement des articles du blog",
  sitesWriteInDocs: "Écrire dans alo Docs",
  sitesOpeningDocs: "Ouverture d’alo Docs…",
  sitesUntitledArticle: "Article sans titre",
  sitesPostCreateFailed: "L’article n’a pas pu être créé. Réessayez.",
  sitesNoPostsTitle: "Aucun article pour le moment",
  sitesNoPostsBody:
    "Commencez un article dans alo Docs. Il reste privé jusqu’à sa publication.",
  sitesColArticle: "Article",
  sitesColUpdated: "Modifié",
  sitesColActions: "Actions",
  sitesEditInDocs: "Modifier dans alo Docs",
  sitesPostStatusDraft: "Brouillon",
  sitesPostStatusPublished: "Publié",
  sitesPublishArticle: "Publier",
  sitesPublishArticleTitle: "Publier l’article",
  sitesPublishArticleSubtitle:
    "Choisissez la présentation de l’article sur votre site public.",
  sitesEditArticleTitle: "Détails de l’article",
  sitesEditArticleSubtitle:
    "Modifiez ce que les lecteurs voient sur votre site.",
  sitesEditArticleDetails: "Modifier les détails",
  sitesSaveArticle: "Enregistrer",
  sitesPostSaveFailed:
    "Les détails de l’article n’ont pas pu être enregistrés. Réessayez.",
  sitesPostUnpublishFailed:
    "L’article n’a pas pu être retiré du site. Réessayez.",
  sitesUnpublishArticle: "Retirer du site",
  sitesUnpublishingArticle: "Retrait en cours…",
  sitesFieldPostTitle: "Titre de l’article",
  sitesFieldPostSlug: "Adresse web",
  sitesPostSlugHint: "Lettres minuscules, chiffres et traits d’union.",
  sitesPostSlugPlaceholder: "mon-article",
  sitesFieldPostExcerpt: "Résumé",
  sitesPostExcerptHint:
    "Une courte introduction affichée sur le blog et dans le flux RSS.",
  sitesFieldPostCover: "Image de couverture",
  sitesPostCoverHint: "Affichée sur le blog et au-dessus de l’article.",
  sitesPostNoCover: "Aucune image",
  sitesPostCoverAdded: "Image ajoutée",
  sitesAddPostCover: "Ajouter une image",
  sitesReplacePostCover: "Remplacer l’image",
  sitesRemovePostCover: "Retirer",
  sitesUploadingPostCover: "Téléversement…",
  sitesPostCoverUploadFailed:
    "L’image de couverture n’a pas pu être téléversée. Réessayez.",
  sitesSeoAction: "Recherche et partage",
  sitesSeoTitle: "Recherche et partage",
  sitesSeoSubtitle:
    "Choisissez comment cette page apparaît dans les résultats et les liens partagés.",
  sitesSeoPreview: "Aperçu du résultat de recherche",
  sitesSeoFieldTitle: "Titre de recherche",
  sitesSeoTitleHint:
    "Laissez vide pour utiliser le titre de la page et le nom du site.",
  sitesSeoFieldDescription: "Description",
  sitesSeoDescriptionHint:
    "Un résumé court et utile pour la recherche et les liens partagés.",
  sitesSeoDescriptionDefault:
    "Ajoutez une description pour présenter cette page.",
  sitesSeoImageHint:
    "Les liens partagés utilisent d’abord l’image principale, puis le logo du site.",
  sitesSeoSave: "Enregistrer les détails",
  sitesSeoSaveFailed:
    "Les détails de recherche n’ont pas pu être enregistrés. Réessayez.",
  sitesStartingPoint: "Comment commencer",
  sitesGenerateChoice: "Générer à partir d’une description",
  sitesTemplateChoice: "Commencer avec un modèle",
  sitesBusinessDescription: "Décrivez votre activité",
  sitesBusinessDescriptionHint:
    "Indiquez votre offre, votre public et le ton souhaité. Vous pourrez tout modifier avant la publication.",
  sitesBusinessDescriptionPlaceholder:
    "Une boulangerie de quartier proposant du pain au levain et des gâteaux de fête aux familles locales…",
  sitesGenerateSite: "Générer le site",
  sitesGenerating: "Préparation de votre brouillon…",
  sitesCreatingSite: "Création du site…",
  sitesGenerationFailed:
    "Votre brouillon n’a pas pu être préparé. Consultez le message du serveur et réessayez.",
  sitesGenerationEmpty:
    "Le brouillon généré ne contient aucune page. Essayez une description plus complète.",
  sitesGenerationUnavailable:
    "La génération n’est pas configurée pour cet espace. Commencez avec un site vierge ou choisissez un modèle ci-dessous.",
  sitesChooseTemplate: "Choisissez un point de départ",
  sitesBlankTemplate: "Site vierge",
  sitesBlankTemplateSummary:
    "Une page d’accueil vide. Vous choisissez vous-même chaque section.",
  sitesTemplatePageCount: (count: number) =>
    count === 1 ? "1 page" : `${count} pages`,
  sitesTemplatesLoading: "Chargement des modèles…",
  sitesTemplatesLoadFailed:
    "Les modèles n’ont pas pu être chargés. Vous pouvez toujours commencer avec un site vierge.",
  sitesTemplatePreviewTitle: (name: string) => `Aperçu de ${name}`,
  sitesTemplatePreviewPages: "Pages de ce modèle",
  sitesTemplatePreviewLoading: "Chargement de l’aperçu…",
  sitesTemplatePreviewFailed:
    "Cet aperçu n’a pas pu être chargé. Vous pouvez tout de même créer le site à partir de ce modèle.",
  sitesTemplatePreviewNote:
    "Une image de la page. Changez de page ci-dessus ; chaque mot et chaque section restent modifiables ensuite.",
  sitesBlankPreviewNote:
    "Vous commencez avec une page d’accueil vide et ajoutez les sections de votre choix.",
  sitesHomePageTitle: "Accueil",
  sitesAiEditTitle: "Décrivez une modification de page",
  sitesAiEditBody:
    "alo prépare une liste à vérifier. Rien ne change avant votre approbation.",
  sitesAiInstruction: "Modification de la page",
  sitesAiInstructionPlaceholder:
    "Rendez l’accueil plus chaleureux et placez les témoignages avant les tarifs…",
  sitesAiPropose: "Préparer les modifications",
  sitesAiPreparing: "Préparation des modifications…",
  sitesAiProposalTitle: "Modifications proposées",
  sitesAiProposalCount: (count: number) =>
    count === 1
      ? "1 modification proposée"
      : `${count} modifications proposées`,
  sitesAiPreviewHint:
    "Comparez la page avant et après, puis choisissez la suite.",
  sitesAiPreviewCompare: "Comparer les modifications proposées",
  sitesInlineTextHint:
    "Cliquez sur n’importe quel texte de l’aperçu pour le modifier sur place. Entrée enregistre, Échap rétablit.",
  sitesInlineTextSaved: "Texte mis à jour.",
  sitesInlineTextUndone: "Modification du texte annulée.",
  sitesInlineTextRedone: "Modification du texte rétablie.",
  sitesInlineTextStale:
    "Ce texte appartient à une section qui a depuis été déplacée ou modifiée. L’aperçu a été actualisé — réessayez la modification.",
  sitesUndoEdit: "Annuler la dernière modification",
  sitesRedoEdit: "Rétablir la dernière modification",
  sitesSectionDragHint:
    "Faites glisser une section pour la déplacer — la page se réorganise pendant le déplacement. Au clavier, sélectionnez une section et maintenez Alt avec la flèche haut ou bas.",
  sitesSectionResizeHint:
    "Certaines sections peuvent changer de forme. Choisissez une taille sous la section dans la liste, ou placez le focus dessus dans l’aperçu et maintenez Alt avec la flèche gauche ou droite.",
  sitesLayoutOf: (control: string) => `Choisir : ${control.toLowerCase()}`,
  sitesSectionResized: (section: string, choice: string) =>
    `${section} : ${choice.toLowerCase()}.`,
  sitesLayoutSplit: "Répartition",
  sitesLayoutColumns: "Colonnes",
  sitesLayoutShape: "Forme",
  sitesLayoutSplitWideImage: "Image plus large",
  sitesLayoutSplitHalf: "Moitiés égales",
  sitesLayoutSplitWideText: "Texte plus large",
  sitesLayoutColumnsTwo: "Deux",
  sitesLayoutColumnsThree: "Trois",
  sitesLayoutColumnsFour: "Quatre",
  sitesLayoutShapeNatural: "Comme importée",
  sitesLayoutShapeWide: "Large",
  sitesLayoutShapeSquare: "Carrée",
  sitesLayoutShapeTall: "Haute",
  sitesSectionOnPage: (section: string, position: number, total: number) =>
    `${section}, section ${position} sur ${total}. Faites-la glisser pour la déplacer, ou maintenez Alt et appuyez sur la flèche haut ou bas.`,
  sitesAiPreviewBefore: "Avant",
  sitesAiPreviewAfter: "Après",
  sitesAiApprove: "Approuver les modifications",
  sitesAiApplying: "Application des modifications…",
  sitesAiDiscard: "Ignorer",
  sitesAiEditFailed:
    "La liste des modifications n’a pas pu être préparée. Réessayez ou modifiez directement les sections.",
  sitesAiApplyFailed:
    "Ces modifications n’ont pas pu être appliquées. Consultez le message du serveur et réessayez.",
  sitesAiAddChange: (section: string, position: number) =>
    `Ajouter ${section} en position ${position}`,
  sitesAiRemoveChange: (section: string) => `Supprimer ${section}`,
  sitesAiMoveChange: (section: string, position: number) =>
    `Déplacer ${section} en position ${position}`,
  sitesAiSettingChange: (section: string) =>
    `Mettre à jour un réglage dans ${section}`,
  sitesAiCopyChange: (section: string) => `Réécrire le texte dans ${section}`,
  sitesAiImproveCopy: "Améliorer ce texte",
  sitesAiCopyActions: "Améliorations du texte",
  sitesAiRewrite: "Réécrire",
  sitesAiShorter: "Raccourcir",
  sitesAiLonger: "Ajouter des détails",
  sitesAiTone: "Ton souhaité",
  sitesAiTonePlaceholder: "Chaleureux et direct",
  sitesAiUseTone: "Changer le ton",
  sitesAiCopyBefore: "Texte actuel",
  sitesAiCopyAfter: "Texte proposé",
  sitesAiCopyFailed:
    "Cette modification n’a pas pu être préparée. Réessayez ou continuez à modifier le texte directement.",
  sitesLoadFailed: "Vos sites web n’ont pas pu être chargés.",
  sitesSiteLoadFailed: "Ce site web n’a pas pu être chargé.",
  sitesSaveFailed: "La modification n’a pas pu être enregistrée.",
  sitesCheckFailed: "L’adresse n’a pas pu être vérifiée.",
  sitesNewSite: "Nouveau site web",
  sitesNoSitesTitle: "Aucun site web pour l’instant",
  sitesNoSitesBody:
    "Créez un site pour votre entreprise et publiez-le à sa propre adresse.",
  sitesColName: "Nom",
  sitesColAddress: "Adresse",
  sitesColStatus: "Statut",
  sitesStatusDraft: "Brouillon",
  sitesStatusLive: "En ligne",
  sitesNewSiteTitle: "Nouveau site web",
  sitesNewSiteSubtitle:
    "Partez d’une description ou choisissez l’un des modèles prêts à l’emploi.",
  sitesFieldName: "Nom du site",
  sitesFieldSubdomain: "Adresse",
  sitesSubdomainHint:
    "Lettres minuscules, chiffres et traits d’union, 3 à 40 caractères — cette adresse deviendra celle du site.",
  sitesSubdomainChecking: "Vérification de la disponibilité…",
  sitesSubdomainAvailable: (subdomain: string) =>
    `« ${subdomain} » est disponible.`,
  sitesSubdomainTaken: (subdomain: string) =>
    `« ${subdomain} » est déjà utilisé.`,
  sitesCreateSite: "Créer le site web",
  sitesCancel: "Annuler",
  sitesBack: "Tous les sites web",
  sitesCollaborators: "Collaborateurs",
  sitesCollaboratorsHint:
    "Invitez des personnes à modifier et publier ce site. Elles ne peuvent pas ouvrir vos e-mails, fichiers ou autres sites.",
  sitesCollaboratorEmail: "Adresse e-mail",
  sitesCollaboratorEmailPlaceholder: "collaborateur@exemple.com",
  sitesInviteCollaborator: "Inviter un éditeur",
  sitesCollaboratorsLoading: "Chargement des collaborateurs…",
  sitesCollaboratorsLoadFailed: "Les collaborateurs de ce site n'ont pas pu être chargés.",
  sitesCollaboratorInviteFailed: "Le collaborateur n'a pas pu être invité.",
  sitesCollaboratorRevokeFailed: "L'accès de ce collaborateur n'a pas pu être retiré.",
  sitesCollaboratorCopyFailed: "Le lien de configuration n'a pas pu être copié. Créez-en un nouveau et réessayez.",
  sitesCollaboratorLinkReady: (email: string) =>
    `Un lien de configuration privé est prêt pour ${email}. Copiez-le et partagez-le en toute sécurité.`,
  sitesCollaboratorAdded: (email: string) => `${email} peut maintenant modifier ce site.`,
  sitesCollaboratorLinkCopied: "Lien de configuration copié.",
  sitesCollaboratorRevoked: (email: string) => `L'accès de ${email} a été retiré.`,
  sitesUndoCollaboratorRevoke: "Annuler",
  sitesNoCollaborators:
    "Vous seul pouvez modifier ce site. Saisissez une adresse ci-dessus pour inviter un premier collaborateur.",
  sitesCollaboratorPending: "Invitation en attente",
  sitesCollaboratorActive: "Peut modifier et publier",
  sitesRefreshCollaboratorLink: "Nouveau lien",
  sitesCopyCollaboratorLink: "Copier le lien",
  sitesRevokeCollaborator: "Retirer l'accès",
  sitesInvitationHeading: "Rejoindre ce site",
  sitesInvitationSubtitle: (site: string) =>
    `Vous avez été invité à modifier et publier ${site}.`,
  sitesInvitationLoading: "Vérification de votre invitation…",
  sitesInvitationLoadFailed:
    "Cette invitation a expiré ou a déjà été utilisée. Demandez un nouveau lien au propriétaire du site.",
  sitesInvitationPassword: "Créer un mot de passe",
  sitesInvitationPasswordHint: "Utilisez au moins 8 caractères.",
  sitesInvitationConfirmPassword: "Confirmer le mot de passe",
  sitesInvitationPasswordMismatch: "Les mots de passe ne correspondent pas.",
  sitesInvitationAccept: "Rejoindre le site",
  sitesInvitationAccepting: "Connexion…",
  sitesInvitationAcceptFailed: "Votre invitation n'a pas pu être acceptée.",
  sitesInvitationDone: "Vous pouvez commencer",
  sitesInvitationDoneBody: (email: string) =>
    `Connectez-vous avec ${email}. Vous ne verrez que les sites partagés avec vous.`,
  sitesInvitationSignIn: "Se connecter à alo",
  sitesPages: "Pages",
  sitesNewPage: "Nouvelle page",
  sitesNoPagesTitle: "Aucune page pour l’instant",
  sitesNoPagesBody:
    "Chaque site commence par une page d’accueil. Ajoutez-en une pour commencer.",
  sitesColPage: "Page",
  sitesColPath: "Chemin",
  sitesHomeBadge: "Accueil",
  sitesNewPageTitle: "Nouvelle page",
  sitesNewPageSubtitle: "Une page contient les sections que vous y empilez.",
  sitesFieldPageTitle: "Titre",
  sitesFieldSlug: "Chemin",
  sitesLanguagesLabel: "Langues du site web",
  sitesEditingLanguage: "Langue d’édition",
  sitesLanguages: "Langues",
  sitesLanguagesHint:
    "Ajoutez les langues proposées aux visiteurs et repérez les pages qui restent à traduire.",
  sitesDefaultLanguage: "Langue par défaut",
  sitesAddLanguage: "Ajouter une langue",
  sitesLanguagePlaceholder: "Code de langue, par exemple fr",
  sitesAddLanguageAction: "Ajouter la langue",
  sitesLanguageDefaultBadge: "Par défaut",
  sitesRemoveLanguage: (language: string) => `Retirer ${language}`,
  sitesLanguageSaveFailed:
    "Les langues du site n’ont pas pu être enregistrées. Vérifiez le code de langue et réessayez.",
  sitesTranslationReady: "Prête",
  sitesTranslationProgress: (translated: number, total: number) =>
    `${translated} page(s) sur ${total} traduite(s)`,
  sitesTranslationAllReady:
    "Toutes les langues activées sont prêtes à être publiées.",
  sitesTranslationPublishHint: (count: number) =>
    `${count} traduction(s) utilise(nt) encore le contenu de secours.`,
  sitesContinueTranslating: "Continuer la traduction",
  sitesTranslationSaveFailed:
    "Cette traduction n’a pas pu être enregistrée. Corrigez les détails indiqués et réessayez.",
  sitesTranslationMissingTitle: (locale: string) =>
    `La traduction ${locale} est manquante`,
  sitesTranslationMissingBody: (requested: string, source: string) =>
    `La version ${source} est affichée comme référence. Copiez-la vers ${requested} pour commencer sans modifier la page source.`,
  sitesCopyTranslation: (source: string, target: string) =>
    `Copier ${source} vers ${target}`,
  sitesTranslationDetails: "Détails de la page traduite",
  sitesTranslationDetailsHint: (locale: string) =>
    `Ce titre, ce chemin et ces détails de recherche sont affichés uniquement aux visiteurs ${locale}.`,
  sitesSaveTranslation: "Enregistrer les détails",
  sitesSlugHint:
    "Lettres minuscules, chiffres et traits d’union. Le chemin de la page d’accueil reste vide.",
  sitesFieldHome: "Ceci est la page d’accueil",
  sitesCreatePage: "Créer la page",
  sitesPageLoadFailed: "Cette page n’a pas pu être chargée.",
  sitesBackToSite: "Toutes les pages",
  sitesSections: "Sections",
  sitesAddSection: "Ajouter une section",
  sitesNoSectionsTitle: "Cette page est encore vide",
  sitesNoSectionsBody:
    "Empilez des sections — une accroche, vos points forts, un formulaire de contact — pour construire la page.",
  // La palette (ADR 0042 §4) : des blocs montrés avec le contenu du client.
  sitesPaletteTitle: "Ajouter une section",
  sitesPaletteHint:
    "Faites glisser un bloc sur la page, ou choisissez sa place et appuyez dessus.",
  sitesPalettePosition: "Emplacement",
  sitesPaletteAtTop: "Tout en haut",
  sitesPaletteAtEnd: "Tout en bas",
  sitesPaletteAfter: (section: string) => `Après ${section}`,
  sitesPaletteAdd: (section: string, position: string) =>
    `Ajouter ${section} — ${position.toLowerCase()}`,
  sitesPaletteDropHere: "Déposez ici pour ajouter à la fin",
  sitesPaletteOwnContent: "Affiché avec votre propre contenu.",
  sitesPalettePreviewTitle: (section: string) => `${section} sur votre site`,
  sitesPaletteLoading: "Remplissage avec votre propre contenu…",
  sitesPaletteFailed:
    "Votre contenu n'a pas pu être chargé : ces blocs ouvrent donc un formulaire.",
  sitesPaletteOpensForm: "Ouvre un formulaire",
  sitesPaletteDone: "Terminé",
  sitesPaletteNeedsWriting:
    "Rien de vôtre ici pour l'instant — celui-ci, c'est vous qui l'écrivez. L'ajouter ouvre un formulaire.",
  sitesPaletteNeedsPicture:
    "Ajoutez une image à ce site et ce bloc se remplira tout seul. L'ajouter maintenant ouvre un formulaire.",
  sitesPaletteNeedsCatalog:
    "Créez d'abord un catalogue — ce bloc en montre le contenu. L'ajouter maintenant ouvre un formulaire.",
  sitesPaletteNeedsCollection:
    "Connectez d'abord une collection — ce bloc en montre les lignes. L'ajouter maintenant ouvre un formulaire.",
  sitesPaletteNeedsBooking:
    "Ajoutez d'abord une prestation réservable — ce bloc la propose. L'ajouter maintenant ouvre un formulaire.",
  sitesPaletteNeedsCode:
    "Le code de ce bloc, c'est vous qui l'écrivez. L'ajouter ouvre un formulaire.",
  sitesAddSectionTitle: (section: string) => `Ajouter ${section}`,
  sitesEditSectionTitle: (section: string) => `Modifier ${section}`,
  sitesSaveSection: "Enregistrer la section",
  sitesMoveUp: (section: string) => `Monter ${section}`,
  sitesMoveDown: (section: string) => `Descendre ${section}`,
  sitesEditSection: (section: string) => `Modifier ${section}`,
  sitesDeleteSection: (section: string) => `Supprimer ${section}`,
  sitesSectionMoved: (section: string, position: number, total: number) =>
    `${section} déplacé en position ${position} sur ${total}.`,
  sitesSectionAdded: (section: string, position: number, total: number) =>
    `${section} ajouté en position ${position} sur ${total}.`,
  sitesConfirmDelete: "Supprimer cette section ?",
  sitesPreview: "Aperçu",
  sitesPreviewTitle: "Aperçu du brouillon",
  sitesPreviewDesktop: "Largeur d’ordinateur",
  sitesPreviewMobile: "Largeur de téléphone",
  sitesPreviewFailed: "L’aperçu n’a pas pu être chargé.",
  sitesSectionNav: "Barre de navigation",
  sitesSectionNavDesc: "Liens en haut de la page.",
  sitesSectionHero: "Accroche",
  sitesSectionHeroDesc: "Le grand titre d’ouverture.",
  sitesSectionFeatures: "Points forts",
  sitesSectionFeaturesDesc: "Une grille de ce que vous proposez.",
  sitesSectionTextImage: "Texte et image",
  sitesSectionTextImageDesc: "Un paragraphe à côté d’une image.",
  sitesSectionGallery: "Galerie",
  sitesSectionGalleryDesc: "Une mosaïque d’images.",
  sitesSectionTestimonials: "Témoignages",
  sitesSectionTestimonialsDesc: "Les mots de clients satisfaits.",
  sitesSectionPricing: "Tarifs",
  sitesSectionPricingDesc: "Vos offres et leurs prix.",
  sitesSectionTeam: "Équipe",
  sitesSectionTeamDesc: "Les personnes derrière l’entreprise.",
  sitesSectionFaq: "Questions fréquentes",
  sitesSectionFaqDesc: "Les questions courantes et leurs réponses.",
  sitesSectionCta: "Appel à l’action",
  sitesSectionCtaDesc: "Une bannière qui invite à cliquer.",
  sitesSectionContactForm: "Formulaire de contact",
  sitesSectionContactFormDesc: "Permettez aux visiteurs de vous écrire.",
  sitesSectionFooter: "Pied de page",
  sitesSectionFooterDesc: "La ligne au bas de la page.",
  sitesCountLinks: (count: number) =>
    count === 1 ? "1 lien" : `${count} liens`,
  sitesCountImages: (count: number) =>
    count === 1 ? "1 image" : `${count} images`,
  sitesCountEntries: (count: number) =>
    count === 1 ? "1 entrée" : `${count} entrées`,
  sitesItemN: (position: number) => `Entrée ${position}`,
  sitesRemoveItem: "Supprimer l’entrée",
  sitesAddLink: "Ajouter un lien",
  sitesAddEntry: "Ajouter une entrée",
  sitesAddImage: "Ajouter une image",
  sitesAddTier: "Ajouter une offre",
  sitesAddMember: "Ajouter une personne",
  sitesAddQuestion: "Ajouter une question",
  sitesFieldHeading: "Titre",
  sitesFieldSubheading: "Sous-titre",
  sitesFieldIntro: "Introduction",
  sitesFieldBody: "Texte",
  sitesFieldItemTitle: "Titre",
  sitesFieldLinkLabel: "Texte du lien",
  sitesFieldLinkHref: "Destination du lien",
  sitesFieldButton: "Bouton",
  sitesFieldPrimaryButton: "Bouton principal",
  sitesFieldSecondaryButton: "Bouton secondaire",
  sitesFieldImage: "Image",
  sitesFieldPhoto: "Photo",
  sitesFieldImageId: "Identifiant de l’image",
  sitesImageIdHint:
    "Téléversez une image ou collez l’identifiant d’une image déjà téléversée.",
  sitesFieldImageAlt: "Description de l’image",
  sitesImageAltHint:
    "Lue à voix haute par les lecteurs d’écran. Dites ce que montre l’image ; si elle ne montre rien d’important, cochez « décorative » ci-dessous.",
  sitesImageAltMissing:
    "Cette image n’a pas encore de description — dites ce qu’elle montre, ou marquez-la comme décorative.",
  sitesImageDecorative: "Décorative — les lecteurs d’écran l’ignorent",
  sitesImageDecorativeHint:
    "Uniquement pour les images qui n’apportent aucune information par elles-mêmes, comme un motif de fond.",
  sitesImageFrameHint:
    "Faites glisser sur l’image pour choisir ce qui reste visible. Au clavier : les flèches déplacent le cadre, majuscule et flèches le redimensionnent.",
  sitesImageFocalHint:
    "Placez le repère rond sur ce qui doit rester visible lorsqu’une mise en page recadre encore l’image.",
  sitesImageFrameAt: (width: number, height: number, left: number, top: number) =>
    `Zone visible : ${width} % sur ${height} % de l’image, à ${left} % du bord gauche et ${top} % du bord supérieur`,
  sitesImageFocalAt: (x: number, y: number) =>
    `Point focal à ${x} % horizontalement et ${y} % verticalement`,
  sitesImageFrameWidth: "Largeur",
  sitesImageFrameHeight: "Hauteur",
  sitesImageFrameLeft: "Gauche",
  sitesImageFrameTop: "Haut",
  sitesImageWholePicture: "Utiliser toute l’image",
  sitesImageWholePictureState: "L’image entière est affichée",
  sitesImageCentreFocal: "Centrer le point focal",
  sitesImageNoPreview:
    "Cette image ne peut pas être affichée ici. Les valeurs ci-dessous la cadrent toujours, et sa description reste inchangée.",
  sitesAiAltWrite: "Proposer une description",
  sitesAiAltImprove: "Améliorer cette description",
  sitesAiAltProposed: "Description proposée",
  sitesAiAltUnseen:
    "Rédigée à partir des textes de cette section — alo n’a pas vu l’image. Vérifiez-la avant de l’approuver.",
  sitesAiAltFailed: "La description n’a pas pu être rédigée.",
  sitesFieldImageSide: "Côté de l’image",
  sitesSideLeft: "Gauche",
  sitesSideRight: "Droite",
  sitesFieldQuote: "Citation",
  sitesFieldAuthor: "Auteur",
  sitesFieldRole: "Fonction",
  sitesFieldTierName: "Nom de l’offre",
  sitesFieldPrice: "Prix",
  sitesFieldPeriod: "Période de facturation",
  sitesFieldTierDescription: "Description",
  sitesFieldTierFeatures: "Ce qui est inclus",
  sitesTierFeaturesHint: "Une ligne par point.",
  sitesFieldHighlighted: "Mettre cette offre en avant",
  sitesFieldMemberName: "Nom",
  sitesFieldBio: "Présentation",
  sitesFieldQuestion: "Question",
  sitesFieldAnswer: "Réponse",
  sitesFieldSuccessMessage: "Message après l’envoi",
  sitesFieldFooterText: "Texte du pied de page",
  sitesContactFormHint:
    "Le formulaire apparaît déjà sur la page ; l’envoi fonctionnera lorsque les formulaires seront disponibles.",
  sitesTheme: "Thème",
  sitesThemeTitle: "Thème du site",
  sitesThemeSubtitle:
    "Choisissez un style, puis ajoutez votre logo et votre favicon.",
  sitesThemeApply: "Appliquer le thème",
  sitesThemeLoadFailed: "Les thèmes n’ont pas pu être chargés.",
  sitesThemePresets: "Couleurs et typographie",
  sitesThemeLogo: "Logo",
  sitesThemeLogoHint:
    "Affiché dans la barre de navigation à la place du nom du site.",
  sitesThemeFavicon: "Favicon",
  sitesThemeFaviconHint:
    "La petite icône affichée dans l’onglet du navigateur.",
  sitesThemeUpload: "Téléverser une image",
  sitesThemeReplace: "Remplacer l’image",
  sitesThemeRemove: "Supprimer l’image",
  sitesThemeSet: "Image téléversée",
  sitesThemeNotSet: "Aucune pour l’instant",
  sitesUploadFailed: "L’image n’a pas pu être téléversée.",
  sitesUploadImage: "Téléverser une image",
  sitesPublish: "Publier",
  sitesPublishChanges: "Publier les modifications",
  sitesUnpublish: "Mettre hors ligne",
  sitesConfirmUnpublish: "Mettre vraiment le site hors ligne ?",
  sitesLiveAtLabel: "Votre site est en ligne à l’adresse",
  sitesGoesLiveAt: (address: string) =>
    `La publication mettra ce site en ligne à l’adresse ${address}.`,
  sitesAddressPreview: (address: string) =>
    `Votre site sera accessible à l’adresse ${address}.`,
  sitesPublishFailed: "Le site n’a pas pu être publié.",
  sitesUnpublishFailed: "Le site n’a pas pu être mis hors ligne.",

  // ---- alo Finance (wave B4, translated at B4.15) -------------------------
  //
  // Le vocabulaire est celui des documents que ces écrans produisent : une
  // *note de frais*, un *relevé bancaire*, un *plan comptable*, une
  // *déclaration de TVA* — pas la glose des mots anglais. Deux règles suivies
  // partout : (1) les statuts d’une note de frais s’accordent au féminin, parce
  // que le sujet est toujours « la note de frais » et jamais un autre document ;
  // (2) aucune phrase interpolant un montant ne fait accorder un participe sur
  // ce montant (« 1,00 € restent dus » serait faux), d’où les tournures
  // invariables « restant à payer », « un écart de … », « nous avons reçu … ».
  moduleFinance: "Finance",
  financeTabExpenses: "Notes de frais",
  financeTabApprovals: "Approbations",
  financeClaimsTable: "Vos notes de frais",
  financeClaimFilters: "Filtres des notes de frais",
  financeChartFilters: "Période du plan comptable",
  financeStatementsTable: "Relevés importés",
  financeChartTableOf: (kind: string) => `Comptes — ${kind}`,
  financePendingClaimsTable: "Notes de frais à décider",
  financeOwedClaimsTable: "Notes de frais à rembourser",
  financeBankSampleTable: "Transactions d’exemple",
  financeBankSettledTable: "Transactions rapprochées",
  financeBankSetAsideTable: "Transactions écartées",
  financeBankFilters: "Filtre par relevé",
  financeReportPeriod: "Période du rapport",
  financeLoadFailed: "Vos notes de frais n’ont pas pu être chargées.",
  financeSaveFailed: "La modification n’a pas pu être enregistrée.",
  financeCancel: "Annuler",
  financeSave: "Enregistrer",
  financeEdit: "Modifier",
  financeDelete: "Supprimer",
  financeActions: "Actions",
  financeShow: "Afficher",
  financeFrom: "Du",
  financeTo: "Au",

  // La note de frais elle-même.
  financeNewClaim: "Nouvelle note de frais",
  financeEditClaim: "Modifier la note de frais",
  financeClaimSubtitle: "Ce que vous avez dépensé, et avec quel argent.",
  financeSpentOn: "Date",
  financeSpentOnHint:
    "Le jour où l’argent est parti, dans votre propre fuseau horaire.",
  financeMerchant: "Commerçant",
  financeMerchantHint: "Qui a été payé — le nom figurant sur le reçu.",
  financeNoMerchant: "Aucun commerçant",
  financeClaimOf: (merchant: string, day: string) => `${merchant}, le ${day}`,
  financeDescription: "Objet de la dépense",
  financeGross: "Total",
  financeVat: "TVA",
  financeVatHint:
    "La TVA indiquée sur le reçu. Laissez vide s’il n’en indique aucune.",
  financeNoVat: "—",
  financeVatRate: "Taux de TVA %",
  financeVatRateHint: "Tel qu’imprimé : 19, 21, 5,5.",
  financeCurrency: "Devise",
  financeCurrencyHint:
    "Laissez vide pour la devise de votre espace de travail.",
  financeProject: "Projet",
  financeProjectHint:
    "Rattachez la note de frais au travail d’un client, pour qu’elle apparaisse dans le coût de ce projet.",
  financeNoProject: "Aucun projet",
  financeMethod: "Payé avec",
  financeMethodHint: "Seul votre propre argent donne lieu à un remboursement.",
  financeMethodPersonal: "Argent personnel",
  financeMethodCard: "Carte de l’entreprise",
  financeMethodCash: "Caisse",
  financeMethodPersonalOption: "Mon propre argent",
  financeMethodCardOption: "La carte de l’entreprise",
  financeMethodCashOption: "La caisse",
  financeAmountInvalid: "Ce n’est pas un montant.",
  financeRateInvalid: "Ce n’est pas un pourcentage.",

  // Où en est une note de frais. Le mot du serveur, dans la langue de la
  // personne — au féminin, puisqu’il s’agit toujours d’une note de frais.
  financeStatus: "Statut",
  financeAnyStatus: "Tous les statuts",
  financeStatusDraft: "Brouillon",
  financeStatusSubmitted: "En attente",
  financeStatusApproved: "Approuvée",
  financeStatusRejected: "Refusée",
  financeStatusReimbursed: "Remboursée",
  financePaidBackOn: (day: string) => `Remboursée le ${day}`,

  // Les verbes.
  financeSubmit: "Transmettre",
  financeWithdraw: "Retirer",
  financeApprove: "Approuver",
  financeReject: "Refuser",
  financeMarkPaidBack: "Marquer comme remboursée",
  financeMarkPaidBackSubtitle: (person: string, amount: string) =>
    `Retour de ${amount} à ${person}.`,
  financeReimbursedOn: "Remboursée le",
  financeReimbursedOnHint:
    "Le jour où l’argent a réellement bougé — c’est le jour de la comptabilisation.",
  financeDeleteTitle: "Supprimer cette note de frais ?",
  financeDeleteBody:
    "La note de frais et ce que vous y avez saisi sont supprimés. C’est irréversible.",
  financeRejectTitle: "Refuser cette note de frais",
  financeRejectBody: (person: string) =>
    `${person} lira ceci, et pourra corriger la note de frais puis la transmettre à nouveau.`,
  financeRejectPlaceholder: "Pourquoi elle revient…",

  // L’écran de celui qui approuve.
  financePerson: "Personne",
  financeCategory: "Catégorie",
  financeUncategorised: "Non classée",
  financeSubmittedAt: "Transmise le",
  financeApprovedAt: "Approuvée le",
  financeOfWhichVat: (amount: string) => `dont ${amount} de TVA`,
  financeWaitingTitle: "En attente d’une décision",
  financeWaitingEmptyTitle: "Rien n’attend",
  financeWaitingEmptyBody:
    "Les notes de frais transmises par vos collègues apparaissent ici, l’achat le plus ancien en premier.",
  financeOwedTitle: "À rembourser",
  financeOwedNote:
    "Les notes de frais approuvées que vos collègues ont payées de leur poche. Une note payée par la carte de l’entreprise est approuvée sans rien devoir à personne : elle n’est pas ici.",
  financeOwedEmptyTitle: "Personne n’attend d’argent",
  financeOwedEmptyBody:
    "Dès que vous approuvez une note de frais que quelqu’un a payée de sa poche, elle attend ici que l’argent reparte.",

  // Ce qu’un employé voit du module en premier.
  financeExpensesEmptyTitle: "Aucune note de frais sur cette période",
  financeExpensesEmptyBody:
    "Enregistrez ce que vous avez dépensé pour le travail — la date, le total du reçu et l’argent qui a payé. Elle reste la vôtre jusqu’à ce que vous la transmettiez.",

  // ---- la banque, et la pile qu’elle laisse ------------------------------
  financeTabBank: "Banque",
  financeTabReconcile: "Rapprochement",
  financeBankLoadFailed: "Les relevés bancaires n’ont pas pu être chargés.",

  // Importer un relevé.
  financeBankImportStatement: "Importer un relevé",
  financeBankImportTitle: "Importer un relevé bancaire",
  financeBankImportSubtitle:
    "Nous lisons d’abord le fichier et vous montrons ce que nous en avons compris. Rien n’est enregistré avant votre accord.",
  financeBankFile: "Fichier du relevé",
  financeBankFileHint:
    "Un fichier CAMT.053 ou MT940 téléchargé depuis votre banque, ou un export CSV.",
  financeBankAccount: "Compte",
  financeBankAccountHint:
    "L’IBAN concerné par ce relevé. Un fichier CAMT.053 ou MT940 l’indique lui-même ; un CSV ne le fait pas.",
  financeBankCurrencyHint:
    "Pour un CSV qui ne l’indique pas. Laissez vide pour la devise de votre espace de travail.",
  financeBankCheckFile: "Vérifier ce fichier",
  financeBankCheckAgain: "Vérifier à nouveau",
  financeBankImport: "Importer",
  financeBankReadFailed: "Ce fichier n’a pas pu être lu.",
  financeBankImportFailed: "Rien n’a été importé.",
  financeBankStale:
    "Vous avez modifié la façon de lire le fichier. Vérifiez-le à nouveau pour voir le résultat.",
  financeBankStaged: (staged: number, duplicates: number) =>
    duplicates === 0
      ? `${staged} opérations importées.`
      : `${staged} opérations importées ; ${duplicates} étaient déjà là et ont été laissées telles quelles.`,

  // Ce que le serveur a fait du fichier.
  financeBankFormat: "Lu comme",
  financeBankSourceCamt: "CAMT.053",
  financeBankSourceMt940: "MT940",
  financeBankSourceCsv: "CSV",
  financeBankRows: "Opérations",
  financeBankRowsRead: (lines: number, rows: number) =>
    `${lines} lignes sur ${rows}`,
  financeBankSkipped: "Lignes qui ne sont pas des opérations",
  financeBankUnbooked: "Pas encore comptabilisées par la banque",
  financeBankPeriod: "Période",
  financeBankEncoding: "Encodage",
  financeBankSampleTitle:
    "Les premières opérations, telles que nous les lisons",
  financeBankSampleTruncated:
    "Seules les premières opérations sont affichées ici. Toutes sont importées.",
  financeBankRowsRefused: (count: number) =>
    count === 1
      ? "Une ligne ne peut pas être lue : rien n’a été importé."
      : `${count} lignes ne peuvent pas être lues : rien n’a été importé.`,
  financeBankRowAt: (line: number) => `Ligne ${line} :`,
  financeBankRowUnknown: "Une ligne :",

  // Nous dire quelle colonne est quoi.
  financeBankMappingTitle: "Quelle colonne est quoi",
  financeBankMappingNote:
    "Nous avons deviné d’après l’en-tête du fichier. Corrigez ce que nous avons mal compris, puis vérifiez le fichier à nouveau.",
  financeBankColumnNone: "Absente de ce fichier",
  financeBankColDate: "Date de comptabilisation",
  financeBankColValueDate: "Date de valeur",
  financeBankColAmount: "Montant (une seule colonne signée)",
  financeBankColDebit: "Argent sorti",
  financeBankColCredit: "Argent entré",
  financeBankColSign: "Le sens de l’opération",
  financeBankColCurrency: "Devise par ligne",
  financeBankColCounterparty: "Qui a été payé, ou qui a payé",
  financeBankColIban: "Leur compte",
  financeBankColRemittance: "Ce qui était écrit sur le paiement",
  financeBankColReference: "La référence propre à la banque",
  financeBankDates: "Dates lues comme",
  financeBankDecimal: "Centimes séparés par",
  financeBankConventionAuto: "Le déduire du fichier",
  financeBankConventionDmy: "Jour/mois/année",
  financeBankConventionMdy: "Mois/jour/année",
  financeBankConventionYmd: "Année-mois-jour",
  financeBankConventionComma: "Une virgule",
  financeBankConventionDot: "Un point",

  // Ce qui a été importé.
  financeBankLines: "Opérations",
  financeBankClosingBalance: "Solde de clôture",
  financeBankImportedAt: "Importé le",
  financeBankEmptyTitle: "Aucun relevé pour l’instant",
  financeBankEmptyBody:
    "Importez un mois depuis votre banque et chaque opération arrive dans une seule pile, en attente d’être rapprochée de la facture qu’elle a payée.",

  // L’écran de rapprochement.
  financeBankStatement: "Relevé",
  financeBankAllStatements: "Tout ce qui n’est pas encore rapproché",
  financeBankToMatchTitle: (count: number) =>
    count === 1
      ? "1 opération à rapprocher"
      : `${count} opérations à rapprocher`,
  financeBankAllMatchedTitle: "Plus rien à rapprocher",
  financeBankAllMatchedBody:
    "Chaque opération des relevés importés est soit attribuée à une facture, soit écartée. Importez un autre mois pour continuer.",
  financeBankCapped:
    "Cette liste est un premier lot, pas la totalité — traitez-la puis rechargez pour voir la suite.",
  financeBankBookedOn: "Comptabilisée le",
  financeBankCounterparty: "Qui",
  financeBankNoCounterparty: "Aucun nom sur le paiement",
  financeBankRemittance: "Référence",
  financeBankCertain: "Certain",
  financeBankThisOne: "C’est celle-ci",
  financeBankNoGuess:
    "Nous n’avons aucune idée de ce dont il s’agit. Choisissez la facture, ou écartez l’opération.",
  financeBankNotOurs: "Pas à nous",
  financeBankPickInvoice: "Choisir une facture",
  financeBankStillOwed: "restant à payer",
  financeBankStillOwedIs: (amount: string) => `${amount} restant à payer`,
  financeBankMatchFailed: "Cette opération n’a pas été attribuée.",
  financeBankUnmatchFailed: "Ce rapprochement n’a pas été annulé.",
  financeBankIgnoreFailed: "Cette opération n’a pas été écartée.",

  // Pourquoi nous pensons qu’une opération a réglé un document.
  financeBankWhyNumberQuoted:
    "notre numéro de facture est écrit sur le paiement",
  financeBankWhyRuleSaved: "ce payeur a déjà été rapproché de cette façon",
  financeBankWhyCustomerNamed: (percent: number) =>
    `le nom sur le paiement ressemble à celui du client (${percent} %)`,
  financeBankWhyWholeAmount: "le montant correspond exactement à ce qui est dû",
  financeBankWhyOnlyDocument: "c’est la seule facture ouverte pour ce montant",
  financeBankWhyBeforeDue: (days: number) =>
    days === 1
      ? "il est arrivé la veille de l’échéance"
      : `il est arrivé ${days} jours avant l’échéance`,
  financeBankWhyAfterDue: (days: number) =>
    days === 1
      ? "il est arrivé le lendemain de l’échéance"
      : `il est arrivé ${days} jours après l’échéance`,
  financeBankWhyPartPayment: (amount: string) =>
    `c’est une partie de la facture — il resterait ${amount}`,

  // Écarter une opération.
  financeBankIgnoreTitle: "Pas à nous de comptabiliser",
  financeBankIgnoreBody:
    "Dites pourquoi, pour que la prochaine personne qui lit ce relevé n’ait pas à le redécouvrir. Frais bancaires, virement privé, doublon.",
  financeBankIgnore: "Écarter",
  financeBankIgnorePlaceholder: "Pourquoi ce n’est pas à nous…",

  // Choisir la facture à la main.
  financeBankPickTitle: "Quelle facture cette opération a-t-elle réglée ?",
  financeBankPickSubtitle: (amount: string) =>
    `Nous avons reçu ${amount}. Dites ce que ce paiement a réglé.`,
  financeBankFindInvoice: "Rechercher une facture",
  financeBankFindInvoiceHint:
    "Par numéro, ou par la référence que votre client lui a donnée.",
  financeBankNoOpenInvoices:
    "Aucune facture émise n’attend encore de paiement.",
  financeBankNoNumber: "Sans numéro",
  financeBankOverdue: "En retard",
  financeBankConfirmMatch: "C’est elle qui a été réglée",

  // Ce qui est déjà traité.
  financeBankUnmatched: "À rapprocher",
  financeBankMatched: "Rapprochées",
  financeBankIgnored: "Écartées",
  financeBankSettledTitle: "Déjà rapprochées",
  financeBankSettledNote:
    "Chacune a enregistré un paiement et déplacé les comptes. En annuler une l’inverse par une écriture qui lui est propre.",
  financeBankUndoMatch: "Annuler",
  financeBankSetAsideTitle: "Écartées",
  financeBankSetAsideNote:
    "Opérations dont quelqu’un a décidé qu’elles ne sont pas à nous.",
  financeBankUndoIgnore: "Remettre dans la pile",

  // ---- alo Finance : le plan comptable ------------------------------------
  financeTabAccounts: "Comptes",
  financeChartLoadFailed: "Le plan comptable n’a pas pu être chargé.",
  financeChartSeeded:
    "Nous vous avons créé un plan comptable neutre. Chacun de ces comptes est à vous : renommez-le ou renumérotez-le — la numérotation de votre comptable ne cassera rien, car la comptabilisation suit le rôle de chaque compte et non son numéro.",
  financeChartEmptyTitle: "Aucun compte pour l’instant",
  financeChartEmptyBody:
    "Le plan comptable est la liste des endroits où l’argent peut se trouver : la banque, ce que les clients vous doivent, ce que vous gagnez, ce que vous dépensez. Rien ne peut être comptabilisé tant qu’il n’y en a pas.",

  financeAccountAdd: "Ajouter un compte",
  financeAccountEdit: "Modifier",
  financeAccountDelete: "Supprimer",
  financeAccountCode: "Numéro",
  financeAccountCodeHint:
    "Le numéro qu’utilise votre comptable. Lettres et chiffres, sans espaces.",
  financeAccountName: "Nom",
  financeAccountRole: "Rôle",
  financeAccountRoleHint:
    "Ce à quoi ce compte sert automatiquement. Les factures, les paiements et les notes de frais trouvent leur compte par son rôle et jamais par son numéro — renuméroter est donc sans risque, et retirer un rôle empêche ces documents de se comptabiliser tant qu’un autre compte ne l’a pas repris.",
  financeAccountType: "Nature",
  financeAccountTypeHint:
    "Ce que le compte contient. Cela détermine le rapport où il apparaît.",
  financeAccountTypeUnset: "Choisissez…",
  financeAccountActive: "Utilisé",
  financeAccountActiveHint:
    "Un compte retiré conserve son historique et son solde et cesse d’être proposé sur les nouveaux documents.",
  financeAccountInUse: "Utilisé",
  financeAccountRetired: "Retiré",
  financeAccountShowRetired: "Afficher les comptes retirés",
  financeAccountMovement: "Mouvement",
  financeAccountPostings: "Écritures",
  financeAccountSystemNote:
    "Nous avons créé ce compte : il ne peut pas être supprimé, car la comptabilisation passe par lui. Renommez-le, renumérotez-le ou retirez-le.",
  financeAccountNewTitle: "Ajouter un compte",
  financeAccountNewBody: "Votre propre ligne dans votre propre plan.",
  financeAccountEditTitle: "Modifier le compte",
  financeAccountEditBody:
    "Renommer et renuméroter sont sans risque à tout moment.",
  financeAccountSaveFailed: "Le compte n’a pas été enregistré.",
  financeAccountDeleteFailed: "Le compte n’a pas été supprimé.",

  // Les cinq natures, deux fois : le mot court pour un en-tête de tableau, et
  // la phrase à laquelle on répond vraiment en en choisissant une.
  financeAccountTypeAsset: "Ce que nous possédons",
  financeAccountTypeLiability: "Ce que nous devons",
  financeAccountTypeEquity: "Capitaux propres",
  financeAccountTypeIncome: "Ce que nous gagnons",
  financeAccountTypeExpense: "Ce que nous dépensons",
  financeAccountTypeAssetLong:
    "Quelque chose que nous possédons ou qui nous est dû — un compte bancaire, la caisse, les créances clients",
  financeAccountTypeLiabilityLong:
    "Quelque chose que nous devons — fournisseurs, impôts, sommes dues au personnel",
  financeAccountTypeEquityLong:
    "La part des associés, et les soldes avec lesquels les comptes ont ouvert",
  financeAccountTypeIncomeLong: "Quelque chose que nous gagnons",
  financeAccountTypeExpenseLong: "Quelque chose que nous dépensons",

  // Les rôles par lesquels une règle de comptabilisation passe.
  financeRoleNone: "Aucun rôle particulier",
  financeRoleAr: "Ce que les clients nous doivent",
  financeRoleAp: "Ce que nous devons aux fournisseurs",
  financeRoleBank: "Le compte bancaire par lequel l’argent passe",
  financeRoleCash: "La caisse",
  financeRoleVatOutput: "La TVA que nous avons facturée et que nous devons",
  financeRoleVatInput: "La TVA que nous avons payée et pouvons récupérer",
  financeRoleRevenue: "Les ventes",
  financeRoleExpenseDefault: "Les frais sans catégorie propre",
  financeRoleEmployeePayable: "Les notes de frais que nous devons au personnel",
  financeRoleFxDiff: "Les écarts de change",
  financeRoleRounding: "Les écarts d’arrondi",
  financeRoleOpeningBalance: "Les soldes avec lesquels les comptes ont ouvert",
  financeRoleSuspense: "L’argent que nous ne savons pas encore où placer",

  // ---- alo Finance : les quatre rapports ----------------------------------
  financeTabReports: "Rapports",
  financeReportPl: "Compte de résultat",
  financeReportBalance: "Bilan",
  financeReportAged: "Qui doit quoi",
  financeReportVat: "Déclaration de TVA",
  financeReportFrom: "Du",
  financeReportTo: "Au",
  financeReportOn: "Au",
  financeReportShow: "Afficher",
  financeReportToday: "Aujourd’hui",
  financeReportThisYear: "Cette année",
  financeReportThisQuarter: "Ce trimestre",
  financeReportLastQuarter: "Trimestre précédent",
  financeReportLastYearEnd: "Fin de l’année dernière",
  financeReportDownloadCsv: "Télécharger le CSV",
  financeReportDownloadFailed: "Le fichier n’a pas pu être téléchargé.",
  financeReportLoadFailed: "Le rapport n’a pas pu être chargé.",
  financeReportBasis: (from: string, to: string) =>
    `Tout ce qui est comptabilisé entre le ${from} et le ${to}, ces deux jours inclus.`,
  financeReportBasisOn: (on: string) =>
    `Tout ce qui est comptabilisé jusqu’au ${on} inclus.`,
  financeReportEmptyTitle: "Rien n’est encore comptabilisé",
  financeReportEmptyBody:
    "Les factures émises, les paiements et les notes de frais approuvées se comptabilisent d’eux-mêmes. Dès que c’est le cas, cela apparaît ici.",
  financeReportAmount: "Montant",
  financeReportTotal: "Total",
  financeReportPrevious: (from: string, to: string) => `${from} – ${to}`,

  // Le compte de résultat.
  financeReportIncome: "Ce que nous avons gagné",
  financeReportIncomeTotal: "Gagné au total",
  financeReportExpense: "Ce que nous avons dépensé",
  financeReportExpenseTotal: "Dépensé au total",
  financeReportProfit: "Bénéfice",
  financeReportLoss: "Perte",

  // Le bilan.
  financeReportAssets: "Ce que nous possédons",
  financeReportAssetsTotal: "Possédé au total",
  financeReportLiabilities: "Ce que nous devons",
  financeReportLiabilitiesTotal: "Dû au total",
  financeReportEquity: "Capitaux propres",
  financeReportEquityTotal: "Capitaux propres au total",
  financeReportResultToDate:
    "Bénéfice ou perte à ce jour, pas encore affecté aux capitaux propres",
  financeReportLiabilitiesEquityTotal:
    "Dettes, capitaux propres et résultat réunis",
  financeReportDifference: "Écart",
  financeReportUnbalanced: (amount: string) =>
    `Ces comptes ne sont pas équilibrés : un écart de ${amount} n’est pas expliqué. Ne déclarez rien à partir de ce bilan — envoyez-le-nous plutôt.`,

  // Qui doit quoi.
  financeReportSide: "Affichage",
  financeReportReceivable: "Ce qu’on nous doit",
  financeReportPayable: "Ce que nous devons",
  financeReportParty: "Qui",
  financeReportBandCurrent: "Pas encore échu",
  financeReportBand1To30: "1–30 jours",
  financeReportBand31To60: "31–60 jours",
  financeReportBand61To90: "61–90 jours",
  financeReportBand90Plus: "Plus de 90 jours",
  financeReportOpenDocuments: (count: number) =>
    count === 1 ? "1 document ouvert" : `${count} documents ouverts`,
  financeReportNothingOwedToUs: "Personne ne vous doit rien",
  financeReportNothingWeOwe: "Vous ne devez rien à personne",
  financeReportAgedEmptyBody:
    "Tous les documents émis de ce côté ont été réglés intégralement.",
  financeReportUnconverted: (count: number) =>
    count === 1
      ? "1 document ne figure dans aucune de ces colonnes : nous n’avons pas de taux de change pour l’exprimer dans votre propre devise."
      : `${count} documents ne figurent dans aucune de ces colonnes : nous n’avons pas de taux de change pour les exprimer dans votre propre devise.`,

  // La déclaration de TVA.
  financeReportVatRate: "Taux",
  financeReportVatBase: "Montant hors TVA",
  financeReportVatTax: "TVA",
  financeReportVatOutput: "TVA que nous avons facturée",
  financeReportVatOutputTotal: "Facturée au total",
  financeReportVatInput: "TVA que nous avons payée",
  financeReportVatInputTotal: "Payée au total",
  financeReportVatUnrated: "Sans taux indiqué",
  financeReportVatPayable: "À payer",
  financeReportVatRefund: "À récupérer",
  financeReportVatNote:
    "Ce sont les chiffres de vos comptes — ventes et achats — c’est-à-dire ce à partir de quoi une déclaration se remplit. Le récapitulatif de TVA sous Facturation montre ce que vous avez facturé, ce qui est une autre question.",

  // ---- l’agent Finance : la proposition de catégories ---------------------
  agentActCategorise: "Proposer des catégories",
  agentCategoriseNote:
    "Examine vos propres notes de frais sans catégorie et en propose une pour chacune, parmi les catégories que vous avez déjà utilisées pour ce commerçant. Rien n’est classé tant que vous ne l’avez pas accepté.",
  agentCategoriseFieldPeriod: "Notes de frais depuis le",
  agentCategoriseSuggested: (count: number): string =>
    count === 1 ? "1 proposition" : `${count} propositions`,
  agentCategoriseNone: "Rien à proposer",
  agentCategoriseConsidered: (count: number): string =>
    count === 1
      ? "1 note de frais examinée"
      : `${count} notes de frais examinées`,
  agentCategoriseEvidence: (times: number): string =>
    times === 1
      ? "déjà comptabilisé ici une fois"
      : `déjà comptabilisé ici ${times} fois`,
  agentCategoriseAccept: "Accepter",
  agentCategoriseDecline: "Non",
  agentCategoriseAccepted: "Acceptée",
  agentCategoriseDeclined: "Refusée",
  agentCategoriseLeftOut: "Laissée de côté",
  agentCategoriseNoMerchant: "Aucun commerçant",
  agentCategoriseFooter:
    "Chaque proposition vous attend — rien n’est comptabilisé, déclaré ni récupéré tant que vous ne l’avez pas acceptée.",
  agentCategoriseFailed:
    "Cela n’a pas pu être calculé — réessayez depuis Finance.",
  agentCategoriseReason: (reason: string): string => {
    switch (reason) {
      case "noMerchant":
        return "aucun commerçant permettant de la reconnaître";
      case "noHistory":
        return "vous n’avez jamais classé ce commerçant";
      case "alreadyProposed":
        return "a déjà une proposition";
      case "declined":
        return "vous avez refusé une proposition ici";
      default:
        // Une raison qu’un serveur plus récent connaît et pas ce client : dire
        // qu’elle a été laissée de côté plutôt que de prétendre le contraire.
        return "laissée de côté";
    }
  },

  // ---- l’agent Finance : les deux réponses --------------------------------
  agentActVatSummary: "Chiffres de TVA",
  agentVatSummaryNote:
    "Lit la TVA que vos comptes portent sur ces jours — taxe facturée, taxe payée, et la différence. Rien n’est déclaré et rien n’est modifié.",
  agentVatFieldPeriod: "Période",
  agentVatCharged: "Facturée sur les ventes",
  agentVatPaid: "Payée sur les achats",
  agentVatOwed: "Vous devez",
  agentVatRefund: "On vous doit",
  agentVatBaseSales: "Chiffre d’affaires",
  agentVatBaseCosts: "Charges",
  agentVatUnrated: "Sans taux",
  agentVatRateRow: (rate: string, base: string): string => `${rate} de ${base}`,
  agentVatNothing: "Rien sur ces jours",
  agentVatFooter:
    "Des chiffres pour une déclaration, pas une déclaration — le dépôt se fait toujours sur votre portail national.",
  agentActFlagAnomalies: "Vérifier les comptes",
  agentAnomalyNote:
    "Lit votre journal sur ces jours et nomme ce qui mérite un second regard, avec les écritures qui sont derrière. N’écrit rien et ne marque rien comme vérifié.",
  agentAnomalyFieldPeriod: "Comptes depuis le",
  agentAnomalyFound: (count: number): string =>
    count === 1 ? "1 point à regarder" : `${count} points à regarder`,
  agentAnomalyNone: "Rien ne ressort",
  agentAnomalyScanned: (count: number): string =>
    count === 1 ? "1 écriture lue" : `${count} écritures lues`,
  agentAnomalyShown: (shown: number, found: number): string =>
    `${shown} sur ${found} affichés`,
  agentAnomalyTruncated:
    "Ces jours contiennent plus d’écritures qu’une seule vérification n’en lit — redemandez sur une période plus courte pour voir le reste.",
  agentAnomalyNotComparable: (count: number): string =>
    count === 1
      ? "1 écriture ne nomme ni client ni fournisseur : elle n’a pas pu être comparée"
      : `${count} écritures ne nomment ni client ni fournisseur : elles n’ont pas pu être comparées`,
  agentAnomalyKind: (kind: string): string => {
    switch (kind) {
      case "duplicate":
        return "Comptabilisé deux fois en une semaine";
      case "unusualAmount":
        return "Différent du reste de ce compte";
      case "missingRecurring":
        return "Un mois sans rien";
      default:
        // Un genre qu’un serveur plus récent connaît et pas ce client : encore
        // une question, jamais rien.
        return "À regarder";
    }
  },
  agentAnomalyTypical: (amount: string): string => `d’habitude ${amount}`,
  agentAnomalyMissingMonth: (month: string): string => `rien en ${month}`,
  agentAnomalyEvidence: "Les écritures qui sont derrière",
  agentAnomalyFooter:
    "Rien n’a été modifié et rien n’a été marqué comme vérifié — chacun de ces points est une question sur des écritures, et la réponse à une question est une écriture de correction.",

  // ---- alo Inventaire (B5.09a–c, B5.10 ; traduit en B5.11) ------------------
  //
  // Le vocabulaire est celui d’un magasin, pas d’un grand livre : « en stock »,
  // « nous payons ». Trois choix tenus partout dans ce bloc. Les motifs d’un
  // mouvement et d’un ajustement sont des **noms** (« Réception », « Casse »)
  // et non des participes, parce qu’un participe devrait s’accorder avec une
  // marchandise dont le genre n’est pas connu de la phrase. Les **états d’une
  // commande** s’accordent, eux, au féminin : leur sujet est toujours « la
  // commande ». Et rien ici n’énonce une quantité, une valeur ou une règle qui
  // appartient au serveur — un refus est affiché dans la phrase du serveur.
  moduleInventory: "Inventaire",
  inventoryTabCatalog: "Catalogue",
  inventoryTabStock: "Stock",
  inventoryLoadFailed: "Votre catalogue n’a pas pu être chargé.",
  inventorySaveFailed: "La modification n’a pas pu être enregistrée.",
  inventoryHistoryFailed: "Cet historique n’a pas pu être chargé.",
  inventoryClose: "Fermer",
  inventoryEdit: "Modifier",
  inventoryArchive: "Archiver",
  inventoryRestore: "Restaurer",
  inventoryArchived: "archivé",
  inventoryColActions: "Actions",
  inventoryNoMatches: "Rien ici ne correspond à ce que vous avez saisi.",

  // Le catalogue : la liste de prix vue comme des choses.
  inventoryNewProduct: "Nouveau produit",
  inventorySearchCatalog: "Rechercher par nom, code ou code-barres",
  inventoryStockedOnly: "Articles stockés seulement",
  inventoryShowArchived: "Afficher les archivés",
  inventoryCatalogEmptyTitle: "Votre catalogue est vide",
  inventoryCatalogEmptyBody:
    "Un produit est ici une seule fiche : ce que vous le facturez, ce que vous le payez et — si c’est quelque chose que vous gardez en rayon — la quantité que vous en avez. Ajoutez le premier et il pourra figurer sur une facture et entrer en magasin le jour même.",
  inventoryColProduct: "Produit",
  inventoryColSku: "Code",
  inventoryColBarcode: "Code-barres",
  inventoryColOnHand: "En stock",
  inventoryColPurchasePrice: "Nous payons",
  inventoryColSalePrice: "Nous facturons",
  inventoryColVatRate: "TVA",
  inventoryTypeStocked: "Stocké",
  inventoryTypeService: "Service",
  inventoryNotStocked: "—",
  inventoryArchiveProductConfirm: (name: string) =>
    `Archiver ${name} ? Le produit reste sur tous les documents déjà établis et cesse d’être proposé sur les nouveaux. Vous pouvez le restaurer à tout moment.`,

  // Les champs de la fiche produit, partagés avec la liste de prix de
  // Facturation. Les deux indications qui comptent sont celles qui portent sur
  // une règle du serveur : la clé de contrôle d’un code-barres, et ce que
  // « stocké » décide.
  inventoryFieldSku: "Code (SKU)",
  inventorySkuHint:
    "Votre propre code pour cet article. Unique parmi vos produits ; laissez-le vide si vous n’en avez pas.",
  inventoryFieldBarcode: "Code-barres",
  inventoryBarcodeHint:
    "Le GTIN inscrit sur le carton. Sa clé de contrôle est vérifiée : un code mal saisi est refusé ici plutôt que découvert quand le mauvais article part.",
  inventoryFieldPurchasePrice: "Prix d’achat",
  inventoryPurchasePriceHint: "Ce que vous le payez, dans votre propre devise.",
  inventoryFieldDefaultSupplier: "Fournisseur habituel",
  inventoryDefaultSupplierHint:
    "Auprès de qui cet article est normalement acheté. C’est le point de départ d’une proposition de réapprovisionnement.",
  inventoryNoSupplier: "Personne en particulier",
  inventoryFieldStocked: "Stock",
  inventoryStockedLabel: "Suivre une quantité de cet article",
  inventoryStockedHint:
    "Seul un produit stocké peut se déplacer d’un endroit à un autre. Un service ne peut être ni réceptionné, ni livré, ni compté — et dès qu’un mouvement a eu lieu, cette case ne peut plus être décochée.",

  // La liste des stocks, et ce que ses chiffres veulent dire.
  inventorySearchStock: "Rechercher par produit, code ou endroit",
  inventoryFilterLocation: "Endroit",
  inventoryAllLocations: "Partout",
  inventoryShowCounterparties: "Afficher les contreparties",
  inventoryCounterpartiesNote:
    "Les fournisseurs, les clients, les ajustements et la production sont des contreparties, pas des endroits : ils sont l’autre bout de chaque mouvement. Quand ils sont affichés, le total ci-dessous s’approche de zéro — c’est à cela que ressemble un grand livre qui se boucle, pas un entrepôt vide.",
  inventoryStockEmptyTitle: "Rien n’est encore en rayon",
  inventoryStockEmptyBody:
    "Le stock apparaît ici dès que quelque chose bouge : une commande d’achat que vous réceptionnez, une livraison que vous expédiez, ou un ajustement que vous saisissez à la main. Il n’y a aucune quantité à taper — ce qui est ici est la somme de tout ce qui s’est passé.",
  inventoryColLocation: "Endroit",
  inventoryColValue: "Valeur",
  inventoryColLastMove: "Dernier mouvement",
  inventoryOpenHistory: "Historique",
  inventoryReferenceValue: (total: string) =>
    `${total} aux prix d’achat du jour — un chiffre de référence pour ce qui est listé, pas un solde comptable.`,

  // L’historique des mouvements : de → vers, combien, pourquoi, quel document.
  inventoryHistoryTitle: (product: string) => `${product} — mouvements`,
  inventoryHistorySubtitle: (place: string) =>
    `Tout ce qui est entré ou sorti de ${place}.`,
  inventoryHistoryEmpty: "Rien n’est encore entré ni sorti de cet endroit.",
  inventoryHistoryCapped: (limit: number) =>
    `Les ${limit} mouvements les plus récents sont affichés. Les plus anciens restent enregistrés.`,
  inventoryColWhen: "Quand",
  inventoryColMovement: "De → vers",
  inventoryColQuantity: "Quantité",
  inventoryColWhy: "Motif",
  inventoryColDocument: "Document",
  inventoryNoDocument: "À la main",

  // Ce qu’est un endroit. Les quatre contreparties portent le nom qu’elles ont
  // pour un magasin, pas celui qu’emploie le protocole.
  inventoryKindStock: "Entrepôt",
  inventoryKindTransit: "En transit",
  inventoryKindSupplier: "Fournisseur",
  inventoryKindCustomer: "Client",
  inventoryKindAdjust: "Ajustement",
  inventoryKindProduction: "Production",

  // Pourquoi quelque chose a bougé. Des noms, jamais des participes.
  inventoryReasonReceipt: "Réception",
  inventoryReasonDelivery: "Livraison",
  inventoryReasonTransfer: "Transfert",
  inventoryReasonAdjustment: "Ajustement",
  inventoryReasonReturn: "Retour",
  inventoryReasonShrinkage: "Démarque",
  inventoryReasonCount: "Comptage",

  // Le motif donné pour un ajustement saisi à la main. Des noms, pour la même
  // raison : « Endommagé » devrait s’accorder avec une marchandise que la
  // phrase ne connaît pas.
  inventoryAdjustDamaged: "Casse",
  inventoryAdjustLost: "Perte",
  inventoryAdjustFound: "Excédent",
  inventoryAdjustExpired: "Péremption",
  inventoryAdjustTheft: "Vol",
  inventoryAdjustSample: "Échantillon",
  inventoryAdjustCorrection: "Correction",

  // ---- les deux documents de commande (B5.09b) ------------------------------
  //
  // Une phrase qui précède un acte irréversible dit ce qu’elle va faire, jamais
  // « êtes-vous sûr ». Passer une commande tire un numéro d’une série sans trou
  // et écrit une lettre ; enregistrer une arrivée déplace de la marchandise
  // réelle et établit une facture fournisseur. Aucun de ces actes ne s’annule.
  inventoryTabPurchasing: "Achats",
  inventoryTabSales: "Commandes clients",
  inventoryOrdersLoadFailed: "Ces commandes n’ont pas pu être chargées.",
  inventoryOrderLoadFailed: "Cette commande n’a pas pu être chargée.",
  inventoryDraftOrder: "Brouillon",
  inventoryDraftInvoice: "Facture en brouillon",
  inventoryOrderLate: "En retard",
  inventoryFilterStatus: "État",
  inventoryAllStatuses: "Tous les états",
  inventoryNoOrdersInState: "Aucune commande dans cet état",
  inventoryCancelAction: "Annuler",

  // Le nom d’un état. « Annulée » est partagé : une commande abandonnée est
  // abandonnée, quel que soit le sens dans lequel allait la marchandise.
  inventoryOrderStatusCancelled: "Annulée",
  inventoryPoStatusDraft: "Brouillon",
  inventoryPoStatusSent: "Passée",
  inventoryPoStatusPartial: "Partiellement reçue",
  inventoryPoStatusReceived: "Reçue",
  inventorySoStatusDraft: "Brouillon",
  inventorySoStatusConfirmed: "Confirmée",
  inventorySoStatusPartial: "Partiellement livrée",
  inventorySoStatusDelivered: "Livrée",

  // Les deux listes.
  inventorySearchPurchaseOrders:
    "Rechercher par numéro, fournisseur ou référence",
  inventorySearchSalesOrders: "Rechercher par numéro, client ou référence",
  inventoryNewPurchaseOrder: "Nouvelle commande d’achat",
  inventoryNewSalesOrder: "Nouvelle commande client",
  inventoryPurchaseOrdersEmptyTitle: "Vous n’avez encore rien commandé",
  inventoryPurchaseOrdersEmptyBody:
    "Une commande d’achat enregistre ce que vous avez demandé à un fournisseur. Établissez-la en brouillon, passez-la quand vous êtes prêt, puis enregistrez ce qui arrive en face — le grand livre des stocks s’écrit pour vous.",
  inventorySalesOrdersEmptyTitle: "Aucun client n’a encore commandé",
  inventorySalesOrdersEmptyBody:
    "Une commande client enregistre ce qu’un client vous a demandé. Établissez-la en brouillon, confirmez-la pour lui donner un numéro, puis enregistrez chaque expédition à mesure qu’elle part — la facture ne porte que ce qui est réellement parti.",
  inventoryColOrder: "Commande",
  inventoryColSupplier: "Fournisseur",
  inventoryColCustomer: "Client",
  inventoryColExpected: "Date attendue",
  inventoryColPromised: "Date promise",
  inventoryColState: "État",
  inventoryColTotal: "Total",

  // Le carnet de commandes.
  inventoryTabOrderBook: "Carnet de commandes",
  inventoryOrderBookLoadFailed: "Le carnet de commandes n’a pas pu être chargé.",
  inventoryFilterScope: "Afficher",
  inventoryScopeOpen: "Commandes en cours",
  inventoryScopeAll: "Toutes les commandes",
  inventoryColOrdered: "Commandé",
  inventoryColReserved: "Réservé",
  inventoryColInvoiced: "Facturé",
  inventoryBookTotal: "Toutes commandes confondues",
  inventoryBookMixedCurrencies: (currencies: string) =>
    `Ces commandes sont en ${currencies} : aucun total unique n’a de sens. Les chiffres de chaque commande, eux, sont exacts.`,
  inventoryBookQtyHint: (qtyMilli: string) => `${qtyMilli} restant à livrer`,
  inventoryOrderBookEmptyTitle: "Rien n’est en attente",
  inventoryOrderBookEmptyBody:
    "Le carnet de commandes montre ce que vos clients attendent et ce qu’il vous reste à leur facturer. Confirmez une commande client et elle y figure jusqu’à ce que tout soit livré et facturé.",
  inventoryOrderBookEmptyAllTitle: "Aucune commande n’a été créée",
  inventoryOrderBookEmptyAllBody:
    "Rien n’a encore été vendu, pas même un brouillon. Le carnet se remplira au fur et à mesure.",

  // Le document.
  inventoryBackToPurchaseOrders: "Toutes les commandes d’achat",
  inventoryBackToSalesOrders: "Toutes les commandes clients",
  inventoryCreateDraft: "Créer le brouillon",
  inventorySaveDraft: "Enregistrer",
  inventoryPrintOrder: "Imprimer",
  inventoryUnsavedNotice:
    "Ces modifications ne sont pas encore enregistrées : les totaux ci-dessous sont les derniers que le serveur a calculés.",
  inventoryOrderFrozenNotice:
    "Cette commande a été passée. Elle porte un numéro que le fournisseur détient : elle ne peut plus être modifiée — enregistrez ce qui arrive en face, ou annulez-la.",
  inventorySalesOrderFrozenNotice:
    "Cette commande a été confirmée. Elle porte un numéro que le client détient : elle ne peut plus être modifiée — enregistrez chaque expédition à mesure qu’elle part.",
  inventoryFixLinesFirst:
    "Une des lignes n’est pas terminée. Corrigez-la et enregistrez à nouveau.",
  inventoryOrderNeedsSupplier:
    "Choisissez le fournisseur auprès de qui cette commande est passée.",
  inventoryOrderNeedsCustomer:
    "Choisissez le client pour qui cette commande est établie.",
  inventoryPickSupplier: "Choisir un fournisseur",
  inventoryPickCustomer: "Choisir un client",
  inventorySupplierHint:
    "Auprès de qui vous commandez. Cela ne peut plus être changé une fois la commande passée.",
  inventoryCustomerHint:
    "Pour qui la commande est établie. Cela ne peut plus être changé une fois la commande confirmée.",
  inventoryExpectedHint:
    "Le jour où vous attendez la marchandise. Une commande qui le dépasse est signalée en retard.",
  inventoryPromisedHint:
    "Le jour où vous avez promis la marchandise. Une commande qui le dépasse est signalée en retard.",
  inventoryFieldReference: "Référence",
  inventoryReferenceHint:
    "Votre propre référence pour cette commande — un chantier, un projet, un numéro de dossier.",
  inventoryFieldOrdered: "Passée le",
  inventoryFieldConfirmed: "Confirmée le",
  inventoryFieldNote: "Note",
  inventoryOrderNoteHint:
    "Ce que l’autre partie doit lire. C’est imprimé sur la commande.",

  // La grille des lignes. Le vocabulaire est celui d’un document, parce que
  // ces lignes en deviennent un.
  inventoryLines: "Lignes",
  inventoryAddLine: "Ajouter une ligne",
  inventoryNoLines: "Aucune ligne pour l’instant.",
  inventoryColDescription: "Désignation",
  inventoryColUnit: "Unité",
  inventoryColUnitPrice: "Prix unitaire",
  inventoryColNet: "Net",
  inventoryColReceived: "Reçu",
  inventoryColDelivered: "Livré",
  inventoryColOutstanding: "Reste",
  inventoryColToBill: "À facturer",
  inventoryPickProduct: "Depuis le catalogue",
  inventoryDescriptionPlaceholder: "Ce qui est commandé",
  inventoryUnitPlaceholder: "pièce",
  inventoryQtyPlaceholder: "1",
  inventoryAmountPlaceholder: "0,00",
  inventoryRatePlaceholder: "0",
  inventoryRemoveLine: "Supprimer la ligne",
  inventoryLineNeedsDescription: "Indiquez à quoi correspond cette ligne.",
  inventoryNotAQuantity: "Ce n’est pas une quantité.",
  inventoryNotAnAmount: "Ce n’est pas un montant.",
  inventoryNotARate: "Ce n’est pas un taux.",

  // Passer la commande : un seul acte, et la phrase en énonce les trois parts.
  inventorySendOrder: "Passer la commande",
  inventorySendOrderConfirm:
    "Ceci attribue son numéro à la commande, la fige définitivement, et dépose dans vos brouillons la lettre d’accompagnement avec la commande imprimée en pièce jointe. Rien n’est envoyé tant que vous ne l’envoyez pas vous-même.",
  inventoryOrderPlacedNotice: (to: string, file: string) =>
    `La commande est passée. Une lettre d’accompagnement pour ${to}, avec ${file} en pièce jointe, attend dans vos brouillons — rien n’a été envoyé.`,
  inventoryConfirmOrder: "Confirmer la commande",
  inventoryConfirmOrderConfirm:
    "Ceci attribue son numéro à la commande et la fige définitivement. Aucun message n’est écrit : prévenir le client est une lettre ordinaire que vous envoyez vous-même.",
  inventoryCancelOrder: "Annuler la commande",
  inventoryCancelOrderConfirm:
    "La commande est conservée et reste consultable, mais plus rien n’est attendu en face.",
  inventoryCancelShortConfirm:
    "Une partie de cette commande a déjà bougé. L’annuler revient à accepter ce qui a été traité jusqu’ici comme étant la totalité, et plus rien ne sera attendu. La commande reste consultable.",
  inventoryDiscardDraft: "Supprimer le brouillon",
  inventoryDiscardDraftConfirm:
    "Ce brouillon n’a pas de numéro et n’a été montré à personne : il est supprimé plutôt qu’annulé.",

  // Enregistrer une expédition, dans un sens comme dans l’autre.
  inventoryReceiveGoods: "Enregistrer une arrivée",
  inventoryDeliverGoods: "Enregistrer une expédition",
  inventoryReceiveTitle: (order: string) => `Ce qui est arrivé pour ${order}`,
  inventoryDeliverTitle: (order: string) => `Ce qui part pour ${order}`,
  inventoryReceiveSubtitle:
    "Chaque ligne s’ouvre sur ce qui reste attendu. Modifiez ce qui manque ; le reste demeure en commande. Une facture fournisseur en brouillon est établie pour ce qui est arrivé.",
  inventoryDeliverSubtitle:
    "Chaque ligne s’ouvre sur ce qui reste à livrer. Modifiez ce qui part maintenant ; le reste demeure sur la commande.",
  inventoryReceiveWhere: "Rangé à",
  inventoryReceiveWhereHint:
    "Où la marchandise a réellement été rangée. Le grand livre des stocks est écrit sur cet endroit.",
  inventoryDeliverWhere: "Prélevé à",
  inventoryDeliverWhereHint:
    "Où la marchandise a été prélevée. Le grand livre des stocks est écrit sur cet endroit.",
  inventoryColThisConsignment: "Cette fois",
  inventoryFulfilNoteHint:
    "Ce qu’a noté la personne qui s’en est occupée — une caisse abîmée, une expédition partielle.",
  inventoryFulfilNeedsPlace: "Choisissez d’abord l’endroit.",
  inventoryFulfilNeedsSomething:
    "Aucune ligne n’indique quoi que ce soit : il n’y a rien à enregistrer.",
  inventoryNoPlaces: "Aucun endroit pour l’instant",
  inventoryBookArrival: "Enregistrer l’entrée",
  inventoryBookConsignment: "Enregistrer la sortie",
  inventoryArrivalBooked:
    "L’arrivée est enregistrée, le grand livre des stocks est écrit, et une facture fournisseur en brouillon attend d’être approuvée.",
  inventoryConsignmentBooked:
    "L’expédition est enregistrée et le grand livre des stocks est écrit.",

  // Ce qui a déjà bougé, et ce qui en a été facturé.
  inventoryArrivals: "Arrivées",
  inventoryNoArrivals: "Rien n’est encore arrivé pour cette commande.",
  inventoryArrivalNo: (n: number) => `Arrivée ${n}`,
  inventoryBillDrafted: "Facture fournisseur en brouillon",
  inventoryConsignments: "Expéditions",
  inventoryNoConsignments: "Rien n’est encore parti pour cette commande.",
  inventoryConsignmentNo: (n: number) => `Expédition ${n}`,
  inventoryRaiseInvoice: "Facturer ce qui est parti",
  inventoryRaisedInvoices: "Factures",
  inventoryNoRaisedInvoices:
    "Rien n’a encore été facturé depuis cette commande.",
  inventoryInvoiceDrafted:
    "Une facture en brouillon a été établie pour ce qui est parti. Elle ne porte aucun numéro tant que personne ne l’émet dans Facturation.",

  // ---- la lecture de code-barres (B5.09c) -----------------------------------
  //
  // Les mots suivent le matériel : une douchette est un clavier, donc le champ
  // est l’essentiel et l’appareil photo n’est qu’une seconde façon de faire.
  inventoryScan: "Scanner",
  inventoryScanTitle: "Scanner un code-barres",
  inventoryScanSubtitle:
    "Scannez dans le champ avec une douchette, ou saisissez le code. Sur un téléphone, vous pouvez utiliser l’appareil photo à la place.",
  inventoryScanFieldCode: "Code-barres",
  inventoryScanPlaceholder: "4006381333931",
  inventoryScanHint:
    "Une douchette saisit le code ici et appuie sur Entrée pour vous. Les espaces et les traits d’union sont ignorés.",
  inventoryScanLookup: "Le trouver",
  inventoryScanFailed: "Ce code n’a pas pu être recherché.",
  inventoryScanWaiting: "En attente d’un code.",
  inventoryScanCameraStart: "Utiliser l’appareil photo",
  inventoryScanCameraStop: "Arrêter l’appareil photo",
  inventoryScanCameraFailed:
    "L’appareil photo n’a pas pu être démarré. Autorisez-y l’accès, ou saisissez le code — une douchette, elle, ne demande aucune autorisation.",
  inventoryScanAiming:
    "Visez le code-barres. La lecture s’arrête dès qu’un code est reconnu.",
  inventoryScanNoCamera:
    "Ce navigateur ne sait pas lire un code-barres avec l’appareil photo. Une douchette fonctionne ici : elle saisit dans le champ ci-dessus.",
  inventoryScanOnHand: (quantity: string) =>
    `${quantity} en stock, tous endroits confondus.`,
  inventoryScanNowhere: "Il n’y en a encore nulle part.",
  inventoryScanServiceNote:
    "C’est un service : il n’y en a aucune quantité à trouver.",
  inventoryScanOpenProduct: "Ouvrir ce produit",
  inventoryScanShowInStock: "L’afficher dans la liste",
  inventoryScanAddProduct: "L’ajouter au catalogue avec ce code-barres",

  // L’agent d’inventaire (ADR 0035, B5.10). Chaque mot maintient qu’un
  // brouillon est un brouillon : la carte ne doit jamais laisser croire qu’un
  // fournisseur a été contacté.
  agentActReorderProposals: "Préparer les réapprovisionnements",
  agentReorderNote:
    "Examine tout ce sur quoi vous êtes sous votre propre minimum et écrit une commande d’achat en brouillon par fournisseur. Rien n’est envoyé — chaque brouillon attend dans vos commandes d’achat que vous le vérifiiez et l’envoyiez.",
  agentActStockAnswer: "Vérifier le stock",
  agentStockAnswerNote:
    "Lit où en est un produit à l’instant : en rayon, en commande, promis à des clients. N’écrit rien et ne réserve rien.",
  agentFieldSupplier: "Fournisseur",
  agentFieldLocation: "Endroit",
  agentFieldProduct: "Produit",
  agentReorderEverySupplier: "Tous les fournisseurs",
  agentReorderEverywhere: "Partout",
  agentReorderShortages: (count: number): string =>
    count === 1 ? "1 sous le minimum" : `${count} sous le minimum`,
  agentReorderNothingShort: "Rien n’est sous son minimum",
  agentReorderDrafted: (count: number): string =>
    count === 1 ? "1 commande en brouillon" : `${count} commandes en brouillon`,
  agentReorderLines: (count: number): string =>
    count === 1 ? "1 ligne" : `${count} lignes`,
  agentReorderLeftOut: "Rien commandé pour",
  agentReorderReason: (reason: string): string => {
    switch (reason) {
      case "noSupplier":
        return "personne ne vous l’a chiffré";
      case "nothingToBuy":
        return "la règle ne demande rien";
      default:
        // Un motif qu’un serveur plus récent connaît et pas ce client : encore
        // visiblement écarté, jamais silencieusement oublié.
        return "écarté";
    }
  },
  // Invariable à dessein : la quantité arrive déjà mise en forme, donc
  // « 1 pièce nécessaire » / « 5 pièces nécessaires » ne peut pas s’accorder.
  agentReorderNeeded: (qty: string, unit: string): string =>
    unit === "" ? `${qty} à commander` : `${qty} ${unit} à commander`,
  agentReorderFooter:
    "Ce sont des brouillons. Aucun fournisseur n’a été contacté et aucun numéro de commande n’a été tiré — ouvrez-en un dans Inventaire pour le vérifier et l’envoyer.",
  agentStockOnHand: "En rayon",
  agentStockOnOrder: "En commande",
  agentStockCommitted: "Promis à des clients",
  agentStockAvailable: "Il reste",
  agentStockNoShelf: "Un service — rien n’est stocké",
  agentStockNowhere: "Nulle part",
  agentStockWatched: "Seuils",
  agentStockMinimum: (min: string, target: string): string =>
    `minimum ${min}, objectif ${target}`,
  agentStockBelowMinimum: "sous le minimum",
  agentStockFooter:
    "Des chiffres à l’instant présent. Rien n’a été commandé et rien n’a été mis de côté.",
  sitesTranslateWholeSite: "Traduire tout le site",
  sitesWholeTranslationPreparing:
    "Préparation de la traduction complète à vérifier…",
  sitesWholeTranslationPrepareFailed:
    "La traduction n’a pas pu être préparée. Rien n’a changé ; traduisez les pages manuellement ou réessayez.",
  sitesWholeTranslationApplyFailed:
    "La traduction n’a pas pu être appliquée. Rien n’a changé ; préparez une nouvelle vérification et réessayez.",
  sitesWholeTranslationReview: (language: string) =>
    `Vérifier la traduction en ${language}`,
  sitesWholeTranslationReviewHint:
    "Comparez chaque page et article. Rien n’est enregistré avant votre approbation.",
  sitesWholeTranslationApprove: "Approuver la traduction",
  sitesTranslationPageKind: "Page",
  sitesTranslationPostKind: "Article",
  sitesCatalogs: "Catalogue",
  sitesCatalogsHint:
    "Ce que ce site propose — plats, chambres, services, formations. Les prix sont figés au moment de la publication.",
  sitesCatalogsLoading: "Chargement du catalogue...",
  sitesCatalogsLoadFailed:
    "Les catalogues n’ont pas pu être chargés. Vérifiez votre connexion et réessayez.",
  sitesCatalogLoadFailed:
    "Ce catalogue n’a pas pu être ouvert. Vérifiez votre connexion et réessayez.",
  sitesNewCatalog: "Nouveau catalogue",
  sitesCatalogNoneTitle: "Rien n’est encore proposé",
  sitesCatalogNoneBody:
    "Un catalogue est la liste que votre site affiche — et, si vous le souhaitez, à partir de laquelle il prend des commandes. Commencez par un nom et une devise ; les articles viennent ensuite.",
  sitesCatalogOrdersOn: "Prend les commandes",
  sitesCatalogOrdersOff: "Sans bon de commande",
  sitesCatalogSettings: "Ce catalogue",
  sitesCatalogSettingsHint:
    "Le nom n’est visible que par vous ; les visiteurs voient les articles. Vos modifications atteignent le site en ligne à la prochaine publication.",
  sitesCatalogName: "Nom du catalogue",
  sitesCatalogCurrency: "Devise",
  sitesCatalogCurrencyHint:
    "Trois lettres, par exemple EUR. La changer relit les prix déjà saisis dans la nouvelle devise — elle ne les convertit pas.",
  sitesCatalogOrders: "Prendre les commandes depuis ce catalogue",
  sitesCatalogOrdersHint:
    "Les visiteurs obtiennent un bon de commande sous la liste. Rien n’est payé sur le site : la commande arrive dans votre boîte de réception et vous la confirmez vous-même. Elle apparaît à la prochaine publication.",
  sitesCatalogCreate: "Créer le catalogue",
  sitesCatalogSave: "Enregistrer le catalogue",
  sitesCatalogSaveFailed: "Le catalogue n’a pas pu être enregistré.",
  sitesCatalogDelete: "Supprimer le catalogue",
  sitesCatalogDeleteConfirm: "Le supprimer, avec tout son contenu",
  sitesCatalogDeleteHint:
    "Les articles et les groupes disparaissent aussi. Les pages déjà publiées continuent d’afficher ce qu’elles affichaient jusqu’à votre prochaine publication.",
  sitesCatalogDeleteFailed: "Le catalogue n’a pas pu être supprimé.",
  sitesCatalogGroups: "Groupes",
  sitesCatalogGroupsHint:
    "Facultatif. Un groupe est un intertitre sur la page — Pains, Chambres, Formations d’une demi-journée.",
  sitesCatalogGroupName: "Nom du groupe",
  sitesCatalogNewGroup: "Nouveau groupe",
  sitesCatalogNewGroupPlaceholder: "Pains",
  sitesCatalogAddGroup: "Ajouter le groupe",
  sitesCatalogGroupRemove: (name: string) => `Retirer le groupe ${name}`,
  sitesCatalogGroupRemoveShort: "Retirer",
  sitesCatalogGroupSaveFailed: "Le groupe n’a pas pu être enregistré.",
  sitesCatalogGroupDeleteFailed: "Le groupe n’a pas pu être retiré.",
  sitesCatalogItems: "Articles",
  sitesCatalogItemsHint:
    "Tout ce que ce catalogue propose, dans l’ordre où la page l’affiche.",
  sitesCatalogAddItem: "Ajouter un article",
  sitesCatalogNoItemsTitle: "Ce catalogue est vide",
  sitesCatalogNoItemsBody:
    "Ajoutez ce que vous proposez. Un nom suffit pour commencer — le prix, la photo et la description peuvent suivre.",
  sitesCatalogNoPrice: "Prix sur demande",
  sitesCatalogEdit: "Modifier",
  sitesCatalogEditItem: (name: string) => `Modifier ${name}`,
  sitesCatalogNewItem: "Nouvel article",
  sitesCatalogSaveItem: "Enregistrer l’article",
  sitesCatalogItemSubtitle:
    "Il apparaît sur le site à votre prochaine publication.",
  sitesCatalogItemName: "Nom",
  sitesCatalogItemHandle: "Identifiant",
  sitesCatalogItemHandlePlaceholder: "D’après le nom",
  sitesCatalogItemHandleHint:
    "Le nom court utilisé dans les liens et sur les commandes. Laissez-le vide et nous le créons à partir du nom.",
  sitesCatalogItemPrice: (currency: string) => `Prix (${currency})`,
  sitesCatalogItemPriceHint:
    "Écrivez-le comme sur une carte — 4.50 ou 4,50. Laissez vide pour « prix sur demande ».",
  sitesCatalogItemPriceNote: "À côté du prix",
  sitesCatalogItemPriceNoteHint:
    "Une courte précision — par nuit, à partir de, par personne.",
  sitesCatalogItemGroup: "Groupe",
  sitesCatalogItemNoGroup: "Sans groupe",
  sitesCatalogItemDescription: "Description",
  sitesCatalogItemPhoto: "Photo",
  sitesCatalogItemPhotoNone: "Pas encore de photo",
  sitesCatalogItemPhotoNoneHint:
    "Un article sans photo apparaît quand même, avec son nom, son prix et sa description.",
  sitesCatalogItemPhotoAdd: "Ajouter une photo",
  sitesCatalogItemPhotoReplace: "Remplacer",
  sitesCatalogItemPhotoRemove: "Retirer la photo",
  sitesCatalogItemPhotoPreview: "La photo de cet article",
  sitesCatalogItemPhotoAlt: "Ce que montre la photo",
  sitesCatalogItemPhotoAltHint:
    "Lu à voix haute par les lecteurs d’écran. Décrivez l’image — pas le nom écrit en dessous.",
  sitesCatalogItemPhotoAltMissing:
    "Personne n’a encore décrit cette photo ; d’ici là, la carte reprend le nom de l’article.",
  sitesCatalogItemAvailability: "Disponibilité",
  sitesCatalogAvailabilityHint:
    "« Épuisé » reste affiché, signalé et non commandable. « Masqué » n’est pas publié du tout.",
  sitesCatalogAvailable: "Disponible",
  sitesCatalogSoldOut: "Épuisé",
  sitesCatalogHidden: "Masqué",
  sitesCatalogItemSaveFailed: "L’article n’a pas pu être enregistré.",
  sitesCatalogItemDelete: "Supprimer",
  sitesCatalogItemDeleteConfirm: "Le supprimer",
  sitesCatalogItemDeleteLabel: (name: string) => `Supprimer ${name}`,
  sitesCatalogItemDeleteConfirmLabel: (name: string) => `Le supprimer : ${name}`,
  sitesCatalogItemDeleteFailed: "L’article n’a pas pu être supprimé.",
  sitesSectionCatalog: "Catalogue",
  sitesSectionCatalogDesc:
    "Ce que vous proposez, avec les prix, depuis votre catalogue.",
  sitesCatalogSectionHeading: "Titre au-dessus",
  sitesCatalogSectionChoose: "Quel catalogue",
  sitesCatalogSectionGroup: "Quel groupe",
  sitesCatalogSectionAllGroups: "Tout le catalogue",
  sitesCatalogSectionGroupHint:
    "Afficher un seul groupe sur cette page — la carte du midi, les chambres doubles — ou tout.",
  sitesCatalogSectionGoneGroup: (handle: string) =>
    `${handle} (n’est plus un groupe)`,
  sitesCatalogSectionOneGroup: (handle: string) => `Un seul groupe : ${handle}`,
  sitesCatalogSectionNoCatalogs: "Ce site n’a pas encore de catalogue",
  sitesCatalogSectionNoCatalogsHint:
    "Un catalogue contient ce que vous proposez, avec ses prix. Créez-en un et cette section pourra l’afficher.",
  sitesCatalogSectionOrdersOn:
    "Ce catalogue prend les commandes : la page publiée porte donc un bon de commande sous la liste. Les commandes arrivent dans la boîte de commandes de ce site.",
  sitesCatalogSectionOrdersOff:
    "Ce catalogue ne prend pas les commandes : la page affiche donc la liste seule. La prise de commandes se règle sur le catalogue, pas sur cette section.",
  // Ce qu’un visiteur peut réserver, et l’agenda dans lequel le rendez-vous
  // est inscrit (S2.13c).
  sitesBookings: "Réservations",
  sitesBookingsHint:
    "Ce qu’un visiteur peut réserver sur ce site — un entretien, une visite, une table. Chaque réservation s’inscrit directement dans l’un de vos agendas.",
  sitesBookingsLoading: "Chargement de ce qui peut être réservé...",
  sitesBookingsLoadFailed:
    "Les prestations réservables n’ont pas pu être chargées. Vérifiez votre connexion et réessayez.",
  sitesNewBooking: "Nouvelle prestation réservable",
  sitesBookingNoneTitle: "Rien n’est encore réservable",
  sitesBookingNoneBody:
    "Une prestation réservable, c’est une chose pour laquelle un visiteur peut prendre un créneau. Indiquez sa durée et vos heures d’ouverture ; les créneaux libres sont calculés d’après votre agenda.",
  sitesBookingNoCalendarTitle: "Aucun agenda où inscrire les rendez-vous",
  sitesBookingNoCalendarBody:
    "Une réservation est un rendez-vous dans l’un de vos agendas : il faut donc un agenda dans lequel vous pouvez ajouter des rendez-vous. Créez-en un dans Agenda et il apparaîtra ici.",
  sitesBookingSettings: "Cette prestation",
  sitesBookingSettingsHint:
    "Tout ce qui est proposé au visiteur. Les modifications atteignent le site en ligne à votre prochaine publication.",
  sitesBookingName: "Ce qui est réservé",
  sitesBookingDescription: "Description",
  sitesBookingWhere: "Où cela se passe",
  sitesBookingWherePlaceholder: "Deuxième étage, sonnez",
  sitesBookingWhereLine: (place: string) => `Où : ${place}`,
  sitesBookingCalendar: "Inscrit dans",
  sitesBookingCalendarHint:
    "Les rendez-vous sont inscrits dans cet agenda, et les moments où vous y êtes déjà occupé ne sont jamais proposés.",
  sitesBookingCalendarReadOnly: (name: string) =>
    `${name} — partagé avec vous en lecture seule`,
  sitesBookingCalendarGone: "Agenda devenu inaccessible",
  sitesBookingCalendarGoneHint:
    "L’agenda dans lequel cette prestation était inscrite n’est plus accessible : il a été supprimé, ou son partage a été retiré. Tant que vous n’en choisissez pas un autre, la page publiée ne propose plus aucun créneau.",
  sitesBookingOpenAgenda: "Ouvrir Agenda pour gérer les rendez-vous",
  sitesBookingLength: "Durée (minutes)",
  sitesBookingBuffer: "Battement après (minutes)",
  sitesBookingNotice: "Délai minimal (minutes)",
  sitesBookingHorizon: "Ouvert à l’avance (jours)",
  sitesBookingTimeZone: "Fuseau horaire",
  sitesBookingTimeZoneHint:
    "L’horloge sur laquelle vos heures d’ouverture sont écrites, sous forme de nom IANA comme Europe/Brussels. Les rendez-vous suivent l’heure lors du changement d’heure.",
  sitesBookingHours: "Vos heures d’ouverture",
  sitesBookingHoursHint:
    "Un agenda vide n’est pas un jour ouvert. Ces plages sont ce qui est proposé ; ce qui figure déjà dans l’agenda en est ensuite retiré.",
  sitesBookingDay: "Jour",
  sitesBookingFrom: "De",
  sitesBookingUntil: "À",
  sitesBookingAddWindow: "Ajouter une plage",
  sitesBookingRemoveWindow: (window: string) => `Retirer ${window}`,
  sitesBookingNoHours:
    "Aucune heure d’ouverture pour l’instant — rien ne peut être réservé.",
  sitesBookingQuestions: "Ce que vous demandez à la réservation",
  sitesBookingQuestionsHint:
    "Le nom et l’adresse e-mail sont toujours demandés et ne figurent pas dans cette liste. N’ajoutez que ce dont cette réservation a besoin.",
  sitesBookingQuestionLabel: "Question",
  sitesBookingQuestionLabelPlaceholder: "Numéro de téléphone",
  sitesBookingQuestionKey: "Enregistré sous",
  sitesBookingQuestionKind: "Type de réponse",
  sitesBookingQuestionText: "Une ligne",
  sitesBookingQuestionLongText: "Plusieurs lignes",
  sitesBookingQuestionPhone: "Numéro de téléphone",
  sitesBookingQuestionChoice: "Un choix dans une liste",
  sitesBookingQuestionOptions: "Les réponses proposées",
  sitesBookingQuestionOptionsPlaceholder: "Coupe, couleur, les deux",
  sitesBookingQuestionRequired: "Réponse obligatoire",
  sitesBookingAddQuestion: "Ajouter une question",
  sitesBookingRemoveQuestion: (question: string) =>
    `Retirer la question ${question}`,
  sitesBookingActive: "Accepter les réservations",
  sitesBookingActiveHint:
    "Désactivée, la prestation reste telle quelle et la page publiée indique qu’elle n’accepte pas de réservation pour le moment.",
  sitesBookingCreate: "Créer la prestation",
  sitesBookingSave: "Enregistrer la prestation",
  sitesBookingSaveFailed: "La prestation réservable n’a pas pu être enregistrée.",
  sitesBookingDelete: "Supprimer la prestation",
  sitesBookingDeleteConfirm: "Supprimer",
  sitesBookingDeleteHint:
    "Les rendez-vous déjà inscrits dans votre agenda restent tels quels — rien ici n’en annule un. Les pages déjà publiées continuent de la proposer jusqu’à votre prochaine publication.",
  sitesBookingDeleteFailed: "La prestation réservable n’a pas pu être supprimée.",
  sitesBookingMinutes: (minutes: number) => `${minutes} minutes`,
  sitesBookingOff: "N’accepte pas de réservation",
  sitesBookingPreview: "Ce que voit un visiteur",
  sitesBookingPreviewHint:
    "L’offre telle que la page publiée l’énonce. Les créneaux libres, eux, sont calculés d’après votre agenda au moment où quelqu’un les demande.",
  sitesBookingUnnamed: "Prestation sans nom",
  sitesBookingAsksNothingExtra:
    "Le visiteur indique son nom et son adresse e-mail.",
  sitesBookingAsksAlso: (questions: string) =>
    `Le visiteur indique son nom et son adresse e-mail, ainsi que : ${questions}.`,
  sitesBookingPublishHint:
    "Elle apparaît sur le site dès qu’une page porte une section de réservation pour elle et que vous publiez.",
  sitesBookingOffPreview:
    "Cette prestation est désactivée : la page indiquera qu’elle n’accepte pas de réservation pour le moment.",
  sitesSectionBooking: "Réservation",
  sitesSectionBookingDesc:
    "Laissez les visiteurs réserver un créneau chez vous, directement dans votre agenda.",
  sitesBookingSectionHeading: "Titre au-dessus",
  sitesBookingSectionChoose: "Ce qui se réserve ici",
  sitesBookingSectionNoServices: "Ce site n’a encore rien à réserver",
  sitesBookingSectionNoServicesHint:
    "Une prestation réservable indique sa durée, vos heures d’ouverture et l’agenda dans lequel elle s’inscrit. Créez-en une et cette section pourra la proposer.",
  sitesBookingSectionOffOption: (name: string) =>
    `${name} (n’accepte pas de réservation)`,
  sitesBookingSectionLength: (minutes: number) =>
    `Le visiteur choisit un créneau libre de ${minutes} minutes. Les créneaux viennent de votre agenda au moment de la demande, pas de cette page.`,
  sitesBookingSectionOff:
    "Cette prestation est désactivée : la page publiée indiquera qu’elle n’accepte pas de réservation pour le moment.",
  sitesBookingSectionGone:
    "La prestation proposée par cette section n’existe plus. Choisissez-en une autre, sinon la prochaine publication sera refusée.",
  // La billetterie (ADR 0041) : des dates qui vendent des places d'un article
  // du tarif. Rien n'est copié : noms et prix sont la réponse de Billing à
  // chaque lecture, et une place vendue est un enregistrement.
  sitesSectionTickets: "Billets",
  sitesSectionTicketsDesc:
    "La porte de votre billetterie. L'offre, les prix et les places restent en direct.",
  sitesTicketSectionHeading: "Titre au-dessus",
  sitesTicketSectionBody: "Vos propres mots au-dessus du lien",
  sitesTicketSectionNoEvents: "Rien n'est encore en vente",
  sitesTicketSectionNoEventsHint:
    "La section publiée mène à votre billetterie. Créez un événement pour qu'il y ait quelque chose à acheter.",
  sitesTicketSectionHint:
    "La section publiée mène à votre billetterie ; événements, prix et places sont lus en direct à l'arrivée du visiteur.",
  sitesTicketSectionOnSale: (count: number) =>
    count === 1 ? "1 événement est en vente." : `${count} événements sont en vente.`,
  sitesTickets: "Billets",
  sitesTicketsLoadFailed:
    "Les événements n'ont pas pu être chargés. Vérifiez votre connexion et réessayez.",
  sitesNoTicketEventsTitle: "Pas encore d'événement",
  sitesNoTicketEventsBody:
    "Un événement vend des places d'un article de votre tarif, à une date. La boutique, le paiement et le décompte des places existent déjà — créez le premier événement et votre site peut le vendre.",
  sitesTicketNoProducts: "Votre tarif est encore vide",
  sitesTicketNoProductsHint:
    "Un événement vend des places d'un article du tarif de Facturation, à son propre prix. Ajoutez d'abord l'article là-bas ; son nom et son prix restent ceux de Facturation et ne sont jamais copiés ici.",
  sitesNewTicketEvent: "Nouvel événement",
  sitesNewTicketEventSubtitle:
    "Une date, ce que vaut une place, et combien de places il y a.",
  sitesTicketCreateSubmit: "Créer l'événement",
  sitesTicketCreateFailed: "L'événement n'a pas pu être créé.",
  sitesTicketEventProduct: "Ce qu'une place vend",
  sitesTicketEventProductHint:
    "Un article de votre tarif. Son nom et son prix sont lus en direct, jamais copiés.",
  sitesTicketProductOption: (name: string, price: string) => `${name} — ${price}`,
  sitesTicketEventStartsAt: "Quand cela commence",
  sitesTicketEventCapacity: "Places",
  sitesTicketEventCapacityHint:
    "Augmenter est toujours permis. Réduire s'arrête aux places déjà vendues ou réservées.",
  sitesTicketCapacityTitle: "Changer les places",
  sitesTicketCapacitySubtitle: (taken: number) =>
    taken === 1
      ? "1 place est déjà vendue ou réservée."
      : `${taken} places sont déjà vendues ou réservées.`,
  sitesTicketCapacitySubmit: "Enregistrer les places",
  sitesTicketCapacityFailed: "Le nombre de places n'a pas pu être changé.",
  sitesTicketChangeCapacity: "Places...",
  sitesTicketDelete: "Supprimer",
  sitesTicketChangeCapacityFor: (event: string) => `Modifier les places de ${event}`,
  sitesTicketDeleteFor: (event: string) => `Supprimer ${event}`,
  sitesTicketDeleteConfirm: "Vraiment supprimer ?",
  sitesTicketDeleteHint:
    "Un événement sans aucune vente disparaît. Dès qu'une place est vendue, l'événement est un enregistrement de la vente et reste.",
  sitesTicketDeleteFailed: "L'événement n'a pas pu être supprimé.",
  sitesTicketWhen: "Quand",
  sitesTicketWhat: "Quoi",
  sitesTicketPrice: "Prix",
  sitesTicketSeats: "Places",
  sitesTicketSeatsCell: (sold: number, remaining: number, capacity: number) =>
    `${sold} vendues · ${remaining} sur ${capacity} restantes`,
  sitesTicketHeld: (held: number) =>
    held === 1 ? "(1 en cours d'achat)" : `(${held} en cours d'achat)`,
  sitesTicketGoneProduct: "Plus au tarif",
  sitesAssistantSuggestedTickets: "Puis-je acheter des billets en ligne ?",
  sitesSectionShop: "Boutique",
  sitesSectionShopDesc:
    "La porte de votre boutique. Articles, prix et stock restent en direct.",
  sitesShopSectionHeading: "Titre au-dessus",
  sitesShopSectionBody: "Vos propres mots au-dessus du lien",
  sitesShopSectionNoItems: "Rien n'est encore en boutique",
  sitesShopSectionNoItemsHint:
    "Le bloc mène à votre page boutique. Mettez un produit en stock en vente sur l'écran Boutique et il y apparaît.",
  sitesShopSectionHint:
    "Le bloc mène à votre page boutique. Articles, prix et stock sont lus en direct — rien n'est enregistré dans la page.",
  sitesShopSectionListed: (count: number) =>
    count === 1 ? "1 produit est en boutique." : `${count} produits sont en boutique.`,
  sitesAssistantSuggestedShop: "Que vendez-vous ?",
  sitesShop: "Boutique",
  sitesShopLoadFailed:
    "La boutique n'a pas pu être chargée. Vérifiez votre connexion et réessayez.",
  sitesShopAddProduct: "Ajouter un produit",
  sitesShopAddSubtitle:
    "Choisissez un produit en stock de votre tarif. Son nom, son prix et son stock restent ceux de Facturation et d'Inventaire — la boutique ne fait que le proposer.",
  sitesShopAddSubmit: "Ajouter à la boutique",
  sitesShopAddFailed: "Le produit n'a pas pu être ajouté.",
  sitesShopProduct: "Quoi vendre",
  sitesShopProductHint:
    "Seuls les produits en stock de votre tarif peuvent être vendus depuis le rayon.",
  sitesShopProductOption: (name: string, price: string, units: number) =>
    units === 1
      ? `${name} — ${price} (1 en rayon)`
      : `${name} — ${price} (${units} en rayon)`,
  sitesShopColWhat: "Quoi",
  sitesShopColPrice: "Prix",
  sitesShopColShelf: "En rayon",
  sitesShopGoneProduct: "Plus au tarif",
  sitesShopNotStocked: "Plus suivi en stock",
  sitesShopUnits: (units: number) => (units === 1 ? "1 unité" : `${units} unités`),
  sitesShopRemove: "Retirer",
  sitesShopRemoveFor: (product: string) => `Retirer ${product} de la boutique`,
  sitesShopRemoveConfirm: "Vraiment retirer ?",
  sitesShopRemoveHint:
    "Retirer ne fait que sortir le produit de la vitrine. Les commandes déjà passées le gardent.",
  sitesShopRemoveFailed: "Le produit n'a pas pu être retiré.",
  sitesShopNoProducts: "Rien en stock à vendre pour l'instant",
  sitesShopNoProductsHint:
    "La boutique vend les produits en stock de votre tarif. Ajoutez-en un dans Facturation (ou laissez la configuration en proposer), recevez du stock, et il apparaît ici.",
  sitesShopEmptyTitle: "Votre vitrine est vide",
  sitesShopEmptyBody:
    "Mettez un produit en stock en vente et vos visiteurs peuvent l'acheter sur votre site, payé sur la page du prestataire de paiement.",
  sitesShopAllListed: "Tous les produits en stock sont déjà en boutique.",
  sitesShopDeliveryRate: (price: string) =>
    `La livraison est facturée ${price} par commande.`,
  sitesShopDeliveryFree: "La livraison est gratuite.",
  sitesCommerceReadOnly:
    "Seul le propriétaire de ce site peut modifier ce qu'il vend et ce qu'il facture — vous pouvez consulter, pas modifier.",
  sitesShopDeliveryChange: "Modifier la livraison…",
  sitesShopDeliveryTitle: "Livraison par commande",
  sitesShopDeliverySubtitle:
    "Un tarif unique par commande, facturé avec les marchandises. La TVA suit les marchandises.",
  sitesShopDeliveryLabel: (currency: string) => `Prix de livraison (${currency})`,
  sitesShopDeliveryHint: "0 signifie que la livraison est gratuite.",
  sitesShopDeliverySave: "Enregistrer la livraison",
  sitesShopDeliveryFailed: "Le prix de livraison n'a pas pu être enregistré.",
  sitesShopSetup: "Configurer la boutique",
  sitesShopSetupSubtitle:
    "Décrivez votre activité et recevez une proposition de tarif, de TVA et de frais de livraison à relire. Rien n'est créé avant votre approbation.",
  sitesShopSetupLoadFailed:
    "L'écran de configuration de la boutique n'a pas pu être chargé. Vérifiez votre connexion et réessayez.",
  sitesShopSetupDescribeLabel: "Que vendez-vous ?",
  sitesShopSetupDescribeHint:
    "Nommez ce que vous vendez et les prix que vous pratiquez. Les prix énoncés sont repris tels quels — tout le reste reste un champ vide ou une supposition signalée, à confirmer par vous.",
  sitesShopSetupPropose: "Proposer une configuration",
  sitesShopSetupProposeFailed: "Aucune configuration n'a pu être proposée. Réessayez.",
  sitesShopSetupUnconfigured:
    "Cet espace de travail n'a pas de fournisseur d'IA configuré, rien ne peut donc être proposé ici — établissez votre tarif à la main.",
  sitesShopSetupManualPath: "Vous préférez le faire à la main ?",
  sitesShopSetupManualTickets: "Gérer les événements à billets",
  sitesShopSetupManualCatalogs: "Gérer les catalogues",
  sitesShopSetupExisting: (count: number) =>
    count === 1
      ? "Votre tarif compte déjà 1 article. Approuver ajoute — rien n'est jamais remplacé."
      : `Votre tarif compte déjà ${count} articles. Approuver ajoute — rien n'est jamais remplacé.`,
  sitesShopSetupProposalTitle: "La proposition",
  sitesShopSetupProposalIntro:
    "Relisez chaque ligne avant d'approuver. Les prix affichés ont été énoncés dans votre description ; les champs vides sont à remplir par vous, et chaque taux de TVA est une supposition à confirmer.",
  sitesShopSetupInclude: (name: string) => `Créer « ${name} »`,
  sitesShopSetupItemName: "Nom",
  sitesShopSetupItemUnit: "Unité",
  sitesShopSetupItemPrice: (currency: string) => `Prix (${currency})`,
  sitesShopSetupVatLabel: "TVA %",
  sitesShopSetupVatGuessBadge: "TVA supposée",
  sitesShopSetupNameMissing:
    "Chaque article inclus doit porter un nom avant l'approbation.",
  sitesShopSetupPriceMissing:
    "Votre description n'énonce pas de prix — saisissez-en un avant d'approuver.",
  sitesShopSetupVatMissing:
    "Saisissez un pourcentage de TVA pour chaque article inclus avant d'approuver.",
  sitesShopSetupKindStock: "Marchandises",
  sitesShopSetupKindDated: "Billets",
  sitesShopSetupKindService: "Service",
  sitesShopSetupShippingTitle: "Livraison",
  sitesShopSetupShippingNotNeeded:
    "Rien dans cette proposition ne s'expédie, il n'y a donc pas de frais de livraison à fixer.",
  sitesShopSetupShippingLabel: (currency: string) =>
    `Frais de livraison fixes par commande (${currency})`,
  sitesShopSetupShippingMissing:
    "Des marchandises s'expédient, mais votre description n'énonce pas de frais de livraison — saisissez-les avant d'approuver.",
  sitesShopSetupShippingCurrent: (price: string) => `Actuellement ${price}.`,
  sitesShopSetupShippingSaved: "Frais de livraison enregistrés.",
  sitesShopSetupShippingFailed:
    "Les frais de livraison n'ont pas pu être enregistrés.",
  sitesShopSetupNothingIncluded:
    "Rien n'est coché — cochez au moins un article à créer.",
  sitesShopSetupApprove: (count: number) =>
    count === 1 ? "Approuver — créer 1 article" : `Approuver — créer ${count} articles`,
  sitesShopSetupRetry: "Réessayer",
  sitesShopSetupDiscard: "Abandonner la proposition",
  sitesShopSetupCreated: "Créé",
  sitesShopSetupCreateFailed: "Cet article n'a pas pu être créé.",
  sitesShopSetupDone: (count: number) =>
    count === 1
      ? "1 article figure désormais à votre tarif."
      : `${count} articles figurent désormais à votre tarif.`,
  sitesShopSetupNextTickets: "Planifier les événements des billets",
  sitesOrders: "Commandes",
  sitesOrdersLoadFailed:
    "Les commandes n’ont pas pu être chargées. Vérifiez votre connexion et réessayez.",
  sitesOrdersExport: "Exporter en CSV",
  sitesOrdersExporting: "Exportation...",
  sitesOrdersExportFailed: "Les commandes n’ont pas pu être exportées.",
  sitesNoOrdersTitle: "Aucune commande pour l’instant",
  sitesNoOrdersBody:
    "Dès qu’une page publiée affiche un catalogue qui prend les commandes, ce que les visiteurs demandent arrive ici — avec le détail, leurs coordonnées et le total.",
  sitesOrderList: "Commandes",
  sitesOrderDetail: "Cette commande",
  sitesOrderFilter: "Afficher",
  sitesOrderFilterAll: "Toutes",
  sitesOrderFilterOption: (label: string, count: number) =>
    `${label} (${count})`,
  sitesOrderFilterEmpty: "Aucune commande dans cet état.",
  sitesOrderStatus: "Où en est cette commande",
  sitesOrderStatusNew: "Nouvelle",
  sitesOrderStatusConfirmed: "Confirmée",
  sitesOrderStatusFulfilled: "Terminée",
  sitesOrderStatusCancelled: "Annulée",
  sitesOrderStatusFailed: "La commande n’a pas pu être déplacée.",
  sitesOrderCatalog: "Depuis",
  sitesOrderPhone: "Téléphone",
  sitesOrderItem: "Article",
  sitesOrderQuantity: "Quantité",
  sitesOrderUnitPrice: "L’unité",
  sitesOrderLineTotal: "Ligne",
  sitesOrderTotal: "Total",
  sitesOrderLinesCaption: "Ce qui a été commandé",
  sitesOrderLineNoPrice: "Sur demande",
  sitesOrderQuotedHint:
    "Un article sans prix n’ajoute rien au total — chiffrez-le vous-même dans votre réponse.",
  sitesOrderLineCount: (count: number) =>
    count === 1 ? "1 article" : `${count} articles`,
  sitesOrderDelete: "Supprimer la commande",
  sitesOrderDeleteConfirm: "La supprimer définitivement",
  sitesOrderDeleteHint:
    "Cette commande contient le nom d’une personne, son téléphone et ce qu’elle a demandé. La suppression retire tout : il n’y a pas de retour en arrière.",
  sitesOrderDeleteFailed: "La commande n’a pas pu être supprimée.",
  sitesCollections: "Collections",
  sitesCollectionsHint:
    "Transformez une table alo Base en cartes réutilisables pour votre site.",
  sitesConnectTable: "Connecter une table",
  sitesCollectionsLoading: "Chargement des collections...",
  sitesCollectionsLoadFailed:
    "Les collections n'ont pas pu être chargées. Vérifiez votre connexion et réessayez.",
  sitesCollectionEmptyTitle: "Connectez votre première table",
  sitesCollectionEmptyBody:
    "Choisissez une alo Base, associez ses colonnes une fois, puis réutilisez ses lignes sur toutes les pages.",
  sitesCollectionNoBasesTitle: "Créez d'abord une alo Base",
  sitesCollectionNoBasesBody:
    "Les collections lisent les lignes d'alo Base. Créez une Base dans Drive, puis revenez la connecter.",
  sitesCollectionOpenDrive: "Ouvrir Drive",
  sitesCollectionName: "Nom de la collection",
  sitesCollectionBase: "alo Base",
  sitesCollectionTable: "Table",
  sitesCollectionChooseBase: "Choisir une Base",
  sitesCollectionChooseTable: "Choisir une table",
  sitesCollectionRows: (count: number) =>
    count === 1 ? "1 ligne" : `${count} lignes`,
  sitesCollectionConnectedTo: (base: string, table: string) =>
    `${base} / ${table}`,
  sitesCollectionSourceUnavailable:
    "Choisissez la Base et la table dont les lignes doivent apparaître sur le site.",
  sitesCollectionEdit: (name: string) => `Modifier ${name}`,
  sitesCollectionMapping: "Associer les colonnes au contenu du site",
  sitesCollectionMappingHint:
    "Le titre est obligatoire. Tout le reste est facultatif et peut être ajouté plus tard.",
  sitesCollectionOptional: "Facultatif",
  sitesCollectionNotMapped: "Ne pas afficher",
  sitesCollectionNoCompatibleField:
    "Cette table a besoin d'une colonne de texte",
  sitesCollectionTitleField: "Titre",
  sitesCollectionSlugField: "Chemin de page",
  sitesCollectionSummaryField: "Résumé",
  sitesCollectionBodyField: "Corps",
  sitesCollectionImageField: "Image",
  sitesCollectionLinkField: "Lien",
  sitesCollectionDateField: "Date de publication",
  sitesCollectionSave: "Enregistrer la collection",
  sitesCollectionSaving: "Enregistrement...",
  sitesCollectionSaveFailed:
    "La collection n'a pas été enregistrée. Rien n'a changé ; vérifiez l'association et réessayez.",
  sitesCollectionDisconnect: "Déconnecter",
  sitesCollectionDisconnectConfirm: "Déconnecter maintenant",
  sitesCollectionDisconnectHint:
    "La Base et toutes ses lignes restent dans Drive.",
  sitesCollectionDisconnectFailed:
    "La collection est toujours connectée. Retirez-la des pages qui l'utilisent, puis réessayez.",
  sitesCollectionPreview: "Lignes actuelles",
  sitesCollectionPreviewHint:
    "Voici exactement ce que la prochaine publication lira dans Base.",
  sitesCollectionPreviewLoading: "Chargement des lignes actuelles de Base",
  sitesCollectionPreviewFailed:
    "Ces lignes n'ont pas pu être prévisualisées. Corrigez dans Base la valeur indiquée par le serveur, puis réessayez.",
  sitesCollectionPreviewSaveTitle: "Enregistrez pour prévisualiser les lignes",
  sitesCollectionPreviewSaveBody:
    "Une fois connectée, chaque ligne est vérifiée ici avec les mêmes règles que le site public.",
  sitesCollectionPreviewEmptyTitle:
    "Cette table n'a pas encore de ligne complète",
  sitesCollectionPreviewEmptyBody:
    "Ajoutez un titre à une ligne dans Base et elle apparaîtra ici automatiquement.",
  sitesCollectionPreviewLinked: "Ouvre un lien",
  sitesSectionCollection: "Collection",
  sitesSectionCollectionDesc:
    "Une grille réutilisable de lignes provenant d'alo Base.",
  sitesCollectionSectionHeading: "Titre de la section",
  sitesCollectionSectionChoose: "Collection à afficher",
  sitesCollectionSectionNoConnections:
    "Connectez une table avant d'ajouter cette section",
  sitesCollectionSectionNoConnectionsHint:
    "La collection reste réutilisable : la même Base peut alimenter plusieurs pages.",

  // Le bloc de code personnalisé, isolé dans un cadre scellé (S2.14b).
  sitesSectionCustomCode: "Code personnalisé",
  sitesSectionCustomCodeDesc:
    "Votre propre HTML, CSS et JavaScript, scellé dans un cadre sans issue.",
  sitesCustomCodeBoundaryTitle: "Ce que ce bloc peut et ne peut pas faire",
  sitesCustomCodeBoundarySealed:
    "Il s’exécute isolé de votre site : il ne peut lire ni la page qui l’entoure, ni vos visiteurs, ni ce qu’ils ont saisi ailleurs.",
  sitesCustomCodeBoundaryNoNetwork:
    "Il n’a aucun accès réseau. Rien ne se charge depuis une autre adresse — ni intégration, ni police, ni script de mesure — et c’est ce qui évite à ce site toute bannière de cookies.",
  sitesCustomCodeBoundaryYours:
    "C’est votre code, publié exactement tel que vous l’avez écrit. Nous ne vérifions pas ce qu’il fait, et l’assistant ne l’écrira ni ne le modifiera.",
  sitesCustomCodeHeadingHint:
    "Affiché par la page au-dessus du bloc, dans la typographie de votre site. Laissez vide pour un bloc qui se suffit à lui-même.",
  sitesCustomCodeFrameTitle: "Ce qu’est ce bloc",
  sitesCustomCodeFrameTitleHint:
    "Lu à voix haute aux visiteurs qui utilisent un lecteur d’écran : « Un minuteur pour la torréfaction en cours », et non « cadre ».",
  sitesCustomCodeHtml: "Balisage",
  sitesCustomCodeHtmlHint:
    "Le corps du bloc. Le document qui l’entoure — sa politique, ses blocs de style et de script — est écrit pour vous.",
  sitesCustomCodeCss: "Style",
  sitesCustomCodeCssHint: "Ne s’applique qu’à l’intérieur de ce bloc. Facultatif.",
  sitesCustomCodeJs: "Script",
  sitesCustomCodeJsHint:
    "Ne s’exécute qu’à l’intérieur de ce bloc, sur l’appareil du visiteur.",
  sitesCustomCodeCapabilities: "Ce que le bloc a le droit de faire",
  sitesCustomCodeCapabilitiesHint:
    "Tout est désactivé tant que vous ne l’activez pas, et seules ces deux autorisations existent.",
  sitesCustomCodeScripts: "Exécuter un script",
  sitesCustomCodeScriptsHint:
    "Sans cela, le bloc n’est que balisage et style : rien ne s’y exécute, quoi qu’il contienne.",
  sitesCustomCodeScriptMissing:
    "Il n’y a encore aucun script à exécuter. Écrivez-en un ou désactivez cette option : une autorisation sans rien derrière est refusée.",
  sitesCustomCodeScriptDropped:
    "Option désactivée : le script ci-dessous ne sera pas enregistré avec le bloc. Réactivez-la pour le conserver.",
  sitesCustomCodeImages: "Afficher les images contenues dans le balisage",
  sitesCustomCodeImagesHint:
    "Pour une image écrite directement dans le balisage. Une image provenant d’une adresse ne peut toujours pas se charger — utilisez une section image pour cela.",
  sitesCustomCodeHeight: "Hauteur sur la page (pixels)",
  sitesCustomCodeHeightHint:
    "Un bloc scellé ne peut pas être mesuré de l’extérieur : c’est donc vous qui indiquez sa hauteur. Entre 40 et 2000.",
  sitesCustomCodeBytes: (used: number, max: number) => `${used} octets sur ${max}`,
  sitesCustomCodeBytesOver: (used: number, max: number) =>
    `${used} octets sur ${max} — trop long pour être enregistré`,
  sitesCustomCodeTotalBytes: (used: number, max: number) =>
    `${used} octets sur ${max} pour l’ensemble du bloc`,
  appLauncherAutoHint:
    "Les applications que vous utilisez le plus, tenues à jour automatiquement",
  meetTitle: "Réunion",
  meetEyebrow: "Votre espace de réunion",
  meetSubtitle: "Démarrez un appel ou rejoignez une réunion déjà en cours.",
  meetHeroTitle: "Réunis en un clic",
  meetHeroText: "Microphone activé, caméra au choix. Vérifiez-les avant que quiconque ne vous voie ou ne vous entende.",
  meetHappeningNow: "En cours",
  meetHappeningHint: "Les réunions que vous pouvez rejoindre sans demander de lien.",
  meetLiveCount: (count: number) => count === 1 ? "1 réunion" : `${count} réunions`,
  meetReady: "Prête",
  meetStartedAt: (time: string) => `Démarrée à ${time}`,
  meetInstantTitle: "Réunion instantanée",
  meetNothingLive: "Aucune réunion en cours",
  meetWhereFrom:
    "Les réunions commencent généralement là où sont les personnes — dans une conversation ou sur une invitation d’agenda. Tout ce qui est en cours et que vous pouvez rejoindre apparaît ici.",
  meetUntitled: "Réunion instantanée",
  meetNotStarted: "Pas encore commencée",
  meetAddToEvent: "Ajouter une réunion",
  meetStart: "Démarrer une réunion",
  meetStartNow: "Démarrer une réunion",
  meetStarting: "Démarrage…",
  meetStartFailed: "La réunion n’a pas pu démarrer. Vérifiez votre connexion et réessayez.",
  meetLoading: "Chargement des réunions",
  meetLoadFailed: "Les réunions n’ont pas pu être chargées",
  meetLoadFailedHint: "Vérifiez votre connexion, puis réessayez. Vous pouvez toujours démarrer une nouvelle réunion.",
  meetRetry: "Réessayer",
  meetBack: "Retour à Meet",
  meetStartedHere: "a démarré une réunion dans cette conversation",
  chatMeetingPreview: "A démarré une réunion",
  meetJoin: "Rejoindre la réunion",
  meetLive: "Réunion en cours",
  meetJoinNow: "Rejoindre maintenant",
  meetReadyGreeting: (name: string) => name ? `Bonjour ${name}` : "Bonjour",
  meetReadyTitle: "Tout est prêt pour participer",
  meetReadyBody: "Vérifiez votre caméra et votre microphone avant de participer.",
  meetReadySafetyTitle: "Votre réunion est sécurisée",
  meetReadySafetyBody: "Seules les personnes invitées ou admises par l’hôte peuvent participer.",
  meetSettingsAfterJoin: "Vous pourrez encore modifier vos réglages après avoir rejoint la réunion.",
  meetGoodConnection: "Bonne connexion",
  meetConnectingStatus: "Connexion en cours",
  meetEnterFullscreen: "Passer en plein écran",
  meetExitFullscreen: "Quitter le plein écran",
  meetMicrophone: "Microphone",
  meetCamera: "Caméra",
  meetJoining: "Connexion en cours…",
  meetLeave: "Quitter",
  meetRecord: "Enregistrer",
  meetRecording: "Enregistrement",
  meetStartRecording: "Démarrer l’enregistrement",
  meetStopRecording: "Arrêter l’enregistrement",
  meetIConsent: "Je consens",
  meetRecordingConsentTitle: "L’enregistrement nécessite le consentement de tous",
  meetRecordingConsentBody: "L’hôte peut commencer lorsque toutes les personnes présentes ont accepté.",
  meetRecordingConsentGiven: "Consentement donné",
  meetConsentCount: (count: number) => `${count} consentement(s)`,
  meetRecordingFailed: "L’action d’enregistrement n’a pas pu être effectuée.",
  meetGenerateMinutes: "Créer le compte rendu",
  meetMinutesTitle: "Compte rendu de réunion",
  meetMinutesActions: "Actions à mener",
  meetMinutesNoActions: "Aucune action n’a été identifiée.",
  meetMinutesFailed: "Le compte rendu nécessite une transcription et un fournisseur d’IA configuré.",
  meetPresentingTitle: "Vous présentez",
  meetPresentingBody: "Toutes les autres personnes voient votre écran partagé. Vous voyez ce rappel discret plutôt qu’un effet de miroir infini.",
  meetClose: "Fermer",
  meetJoinFailed: "Impossible de rejoindre cette réunion.",
  meetJoinProblemTitle: "Nous n’avons pas pu vous connecter",
  meetUnavailableTitle: "Meet a besoin d’une dernière connexion",
  meetRaiseHand: "Lever la main",
  meetLowerHand: "Baisser la main",
  meetReact: "Envoyer une réaction",
  meetInvite: "Inviter",
  meetInviteTitle: "Rejoignez ma réunion alo",
  meetInviteText: "Utilisez ce lien alo pour rejoindre la réunion.",
  meetChatEmptyTitle: "La salle vous écoute",
  meetChatEmptyBody: "Partagez une idée, un lien ou le détail dont chacun aura besoin après l’appel.",
  meetChat: "Chat",
  meetCaptions: "Sous-titres en direct",
  meetCaptionLanguage: "Langue des sous-titres",
  meetCaptionOriginal: "Original",
  meetToolLoading: "Chargement des outils de réunion…",
  meetAgenda: "Ordre du jour",
  meetAgendaHint: "Gardez tout le monde aligné sur la suite.",
  meetAgendaPlaceholder: "Ajouter un point",
  meetPolls: "Sondages",
  meetPollsHint: "Interrogez le groupe et partagez le résultat.",
  meetPollQuestion: "Question",
  meetPollOptionOne: "Première option",
  meetPollOptionTwo: "Deuxième option",
  meetCreatePoll: "Créer le sondage",
  meetNotes: "Notes",
  meetNotesHint: "Des notes partagées qui restent avec la réunion.",
  meetNotesPlaceholder: "Consignez les décisions, le contexte et le suivi…",
  meetFiles: "Fichiers",
  meetFilesHint: "Images et PDF partagés pendant cet appel.",
  meetNoFiles: "Aucun fichier n’a encore été partagé.",
  meetToolsFailed: "Les outils ont changé ailleurs. Rechargez et réessayez.",
  deleteLabel: "Supprimer",
  add: "Ajouter",
  save: "Enregistrer",
  meetCaptionsWaiting: "Les sous-titres apparaîtront lorsque le service de transcription entendra la parole.",
  meetChatTitle: "Chat de l’appel",
  meetChatMessages: "Messages",
  meetChatPeople: (count: number) => `Personnes (${count})`,
  meetChatPlaceholder: "Envoyer un message",
  meetMessageSendFailed: "Ce message n’a pas été enregistré. Vérifiez votre connexion et réessayez.",
  meetEveryone: "Tout le monde",
  meetSendTo: "À :",
  meetChooseRecipient: "Envoyer le message à",
  meetEveryoneHint: "Visible par tous les participants",
  meetPrivateHint: "Seule cette personne le recevra",
  meetPrivate: "Privé",
  meetReplyPrivately: "Répondre en privé",
  meetMessagePrivately: "Message privé",
  meetAttachFile: "Ajouter une image ou un PDF",
  meetAddEmoji: "Ajouter un emoji",
  meetSettings: "Paramètres de l’appel",
  meetDeviceSettings: "Caméra et audio",
  meetDeviceSettingsHint: "Les modifications s’appliquent immédiatement et restent sur cet appareil.",
  meetBackgroundEffects: "Effets d’arrière-plan",
  meetBackgroundEffectsHint: "Gardez l’attention sur vous. L’effet s’applique à la vidéo reçue par tous.",
  meetBackgroundNone: "Aucun",
  meetBackgroundBlur: "Flou",
  meetBackgroundUnsupported: "Le flou d’arrière-plan n’est pas pris en charge par ce navigateur ou cette caméra.",
  meetReconnecting: "Reconnexion à l’appel",
  meetReconnectingHint: "Restez ici — l’audio et la vidéo reprendront automatiquement.",
  meetConnectionLost: "La connexion à l’appel a été interrompue. Essayez de le rejoindre à nouveau.",
  meetPictureInPicture: "Image dans l’image",
  meetSpeaker: "Haut-parleur",
  meetDone: "Terminé",
  meetYou: "Vous",
  meetParticipant: "Participant",
  meetHost: "Hôte",
  meetSpeaking: "Parle",
  meetMuted: "En sourdine",
  meetMuteParticipant: "Couper le micro du participant",
  meetRemoveParticipant: "Retirer le participant",
  meetRemoveParticipantConfirm: (name: string) => `Retirer ${name} de cette réunion ?`,
  meetModerationFailed: "Cette action n’a pas pu être effectuée. Réessayez.",
  meetQuickReplyOne: "👍 Ça marche",
  meetQuickReplyTwo: "Allons-y !",
  meetQuickReplyThree: "Commencer",
  meetJoinPlaceholder: "Saisissez un code de réunion ou un lien alo",
  meetJoinShort: "Rejoindre",
  meetNew: "Nouvelle réunion",
  meetYourSpaceLead: "Votre",
  meetYourSpaceAccent: "espace de réunion",
  meetHeroNewTitle: "Réunissez-vous en un clic",
  meetHeroNewText: "Des appels de qualité avec partage d’écran, chat, réactions et vérification des appareils avant que quiconque vous voie ou vous entende.",
  meetSchedule: "Planifier",
  meetJoinInputInvalid: "Saisissez un lien de réunion alo ou un code de réunion valide.",
  meetUpcoming: "Réunions à venir",
  meetUpcomingHint: "La suite indiquée par votre Agenda.",
  meetRecent: "Réunions récentes",
  meetRecentHint: "Les appels auxquels vous pouviez participer, conservés dans l’historique de l’espace.",
  meetEndedAt: (time: string) => `Terminée ${time}`,
  meetDuration: (minutes: number) => `${minutes} min`,
  meetCalendarUntitled: "Événement sans titre",
  meetSafetyTitle: "Vous gardez le contrôle des accès",
  meetSafetyBody: "L’espace vérifie l’accès avant d’émettre un jeton média. Un code de réunion ne contourne jamais l’autorisation.",
  meetTodaySchedule: "Programme du jour",
  meetOpenAgenda: "Ouvrir Agenda",
  meetNoEventsToday: "Rien d’autre n’est prévu aujourd’hui.",
  meetViewAgenda: "Voir tout l’Agenda",
  meetQuickActions: "Actions rapides",
  meetLinkCopied: "Lien copié",
  meetSomeone: "Quelqu’un",
  meetHandsRaised: (names: string) => `Main levée : ${names}`,
  meetNoEngine:
    "Les réunions ne sont pas encore activées pour cet espace de travail. La réunion est enregistrée et toutes les personnes invitées peuvent la voir — il n’y a simplement pas encore d’endroit où la tenir tant qu’un administrateur n’a pas configuré le serveur de réunion.",
  agendaAgenda: "Agenda",
  agendaCreateEvent: "Créer un événement",
  agendaDay: "Jour",
  agendaDescriptionPlaceholder:
    "Ajoutez des notes, un ordre du jour ou d’autres détails…",
  agendaEditEventSubtitle: "Modifiez les détails de votre événement",
  agendaLocationPlaceholder: "Ajoutez un lieu ou un lien de visioconférence",
  agendaMyCalendars: "Mes agendas",
  agendaNewEventSubtitle: "Créez un nouvel événement dans votre agenda",
  agendaNothingUpcoming: "Rien à venir.",
  agendaOtherCalendars: "Autres agendas",
  agendaTomorrow: "Demain",
  agendaUntitledEvent: "Événement sans titre",
  agendaUpcoming: "À venir",
  driveActions: "Actions",
  driveAdd: "Ajouter",
  driveAddMemberLabel: "Adresse e-mail",
  driveAddMemberPlaceholder: "Ajoutez quelqu’un par e-mail",
  driveColModified: "Modifié",
  driveColName: "Nom",
  driveColSize: "Taille",
  driveCopy: "Faire une copie",
  driveCopyTo: "Copier vers…",
  driveCurrent: "Actuelle",
  driveDeleteForever: "Supprimer définitivement",
  driveDestHint: "L’élément prend les accès de l’endroit où vous le placez.",
  driveDownload: "Télécharger",
  driveKindDoc: "Document",
  driveKindExcel: "Classeur Excel",
  driveKindFolder: "Dossier",
  driveKindSheet: "Sheet",
  driveKindSlides: "Slides (PowerPoint)",
  driveKindWord: "Document Word",
  driveMemberError:
    "Impossible d’ajouter cette personne — vérifiez l’adresse e-mail et votre rôle.",
  driveMemberRoleLabel: "Rôle",
  driveMembers: "Membres",
  driveMove: "Déplacer",
  driveMoveTo: "Déplacer vers…",
  driveMyFiles: "Mes fichiers",
  driveNew: "Nouveau",
  driveNewBase: "Nouvelle base",
  driveNewBasePrompt: "Nommez la nouvelle base",
  driveNewDoc: "Nouveau doc",
  driveNewDocPrompt: "Nommez le nouveau doc",
  driveNewFolder: "Nouveau dossier",
  driveNewFolderPrompt: "Nommez le nouveau dossier",
  driveNewSheetPrompt: "Nommez le nouveau sheet",
  driveNewSpace: "Nouveau Space",
  driveNewSpacePrompt: "Nommez le nouveau Space",
  driveNoVersions: "Aucune version précédente.",
  driveOpen: "Ouvrir",
  driveRemoveMember: "Retirer",
  driveRemoveMemberFor: (who: string): string => `Retirer ${who}`,
  driveRename: "Renommer",
  driveRenamePrompt: "Nouveau nom",
  driveRestore: "Restaurer",
  driveSpaces: "Spaces",
  driveTrash: "Corbeille",
  driveTrashAction: "Mettre à la corbeille",
  driveUpload: "Importer",
  driveUploading: "Importation…",
  driveVersionHistory: "Historique des versions",
  agendaEventCount: (n: number) =>
    n === 1 ? "1 événement" : `${n} événements`,
  driveMembersOf: (name: string) => `Membres de ${name}`,
  driveNameNew: (kind: string): string => `Nommez votre ${kind.toLowerCase()}`,
  drivePurgeConfirm: (name: string) =>
    `Supprimer définitivement « ${name} » ? Cette action est irréversible.`,
  driveRemoveMemberConfirm: (who: string) => `Retirer ${who} de ce Space ?`,
  driveTrashConfirm: (name: string) => `Mettre « ${name} » à la corbeille ?`,
  driveRole: (role: string) =>
    role === "manager"
      ? "Gestionnaire"
      : role === "editor"
        ? "Éditeur"
        : "Lecteur",
  agentActAmIFree: "Vérifier les conflits",
  agentActArchive: "Archiver",
  agentActCatchUp: "Lire ce qui a été dit",
  agentActDraft: "Nouvel e-mail",
  agentActEvent: "Ajouter à l’agenda",
  agentActFindContact: "Rechercher un contact",
  agentActFindFile: "Rechercher dans votre Drive",
  agentActFindInChat: "Rechercher dans les conversations",
  agentActFlag: "Marquer",
  agentActMarkRead: "Marquer comme lu",
  agentActMarkUnread: "Marquer comme non lu",
  agentActMove: "Déplacer vers un dossier",
  agentActReply: "Répondre",
  agentActSend: "Envoyer l’e-mail",
  agentActSnooze: "Reporter",
  agentActTask: "Créer une tâche",
  agentActTrash: "Mettre à la corbeille",
  agentActUnflag: "Retirer la marque",
  agentActWhatsOn: "Lire votre agenda",
  agentFieldDue: "Échéance",
  agentFieldEmail: "E-mail",
  agentFieldEvent: "Événement",
  agentFieldFolder: "Dossier",
  agentFieldLookingFor: "Recherche",
  agentFieldReplyTo: "En réponse à",
  agentFieldRoom: "Conversation",
  agentFieldSubject: "Objet",
  agentFieldTask: "Tâche",
  agentFieldTo: "À",
  agentFieldUntil: "Jusqu’au",
  agentFieldWhen: "Quand",
  agentNoSubject: "(sans objet)",
  agentSendButton: "Envoyer",
  agentSendCaution: "L’e-mail sera envoyé immédiatement — c’est irréversible.",
  chatAddReaction: "Ajouter une réaction",
  chatAgentAddFailed: "Impossible d’ajouter cet agent.",
  chatAgentNothingYet: "Aucune question posée pour l’instant",
  chatAgentRemoveFailed: "Impossible de retirer cet agent.",
  chatAgentTag: "agent",
  chatAgentsAvailable: "Disponibles à ajouter",
  chatAgentsHere: "Agents dans cette conversation",
  chatArchiveAction: "Archiver le canal",
  chatArchiveConfirm: "Archiver",
  chatArchiveFailed: "Impossible d’archiver ce canal.",
  chatArchiveWarning:
    "Rien n’est supprimé. L’historique reste lisible, mais plus personne ne peut y écrire.",
  chatArchived: "Archivé",
  chatArchivedNote:
    "Ce canal est archivé. Son historique reste consultable, mais rien de nouveau ne peut y être envoyé.",
  chatAttach: "Joindre un fichier",
  chatAttachFailed: "Impossible de partager ce fichier.",
  chatBackToList: "Retour aux conversations",
  chatBeginningDm: "C’est le début de votre conversation",
  chatBold: "Gras",
  chatBrowse: "Parcourir les canaux",
  chatBrowseFailed: "Impossible de lister ces canaux.",
  chatBulletList: "Liste à puces",
  chatClose: "Fermer",
  chatCodeBlock: "Bloc de code",
  chatCodeBlockHint: "Insérez un bloc formaté pour du code ou des commandes.",
  chatFormulaHint: "Insérez une formule mathématique.",
  chatFormatting: "Mise en forme du texte",
  chatComposerLabel: "Écrire un message",
  chatCreate: "Créer",
  chatCreateFailed: "Impossible de créer ce canal.",
  chatDecideFailed: "Impossible de trancher.",
  chatDirectMessage: "Message direct",
  chatDmFailed: "Impossible de démarrer cette conversation.",
  chatDropFiles: "Déposez pour partager depuis votre ordinateur",
  chatEditAction: "Modifier",
  chatEditCancel: "Annuler",
  chatEditFailed: "Impossible d’enregistrer cette modification.",
  chatEditLabel: "Modifier ce message",
  chatEditSave: "Enregistrer",
  chatEdited: "modifié",
  chatEmojiNone: "Aucun emoji ne correspond.",
  chatEmojiSearch: "Rechercher un emoji",
  chatFileTrashed: "dans la corbeille de Drive",
  chatFindPerson: "Trouver un collègue",
  chatFindPersonHint: "Tapez au moins deux lettres de son adresse.",
  chatFormatHint: "texte",
  chatFormula: "Formule",
  chatInlineCode: "Code",
  chatInsertEmoji: "Emoji",
  chatItalic: "Italique",
  chatJoin: "Rejoindre",
  chatJoinFailed: "Impossible de rejoindre ce canal.",
  chatJoined: "Ouvrir",
  chatJumpTo: "Aller à une conversation",
  chatLoadFailed: "Impossible de charger ces conversations.",
  chatLoading: "Chargement…",
  chatMembersAndAgents: "Membres et agents",
  chatNewChannel: "Nouveau canal",
  chatNewChannelPlaceholder: "p. ex. lancement-produit",
  chatNewChannelPrompt:
    "Donnez-lui un nom court et évident — c’est ainsi qu’on le rejoint.",
  chatNewDm: "Nouvelle conversation",
  chatNewMessages: "Nouveaux messages",
  chatNoAgentsHere:
    "Aucun agent ici pour l’instant. Ajoutez-en un et mentionnez-le par son nom.",
  chatNoChannelsHint:
    "Créez un canal pour une équipe ou un sujet : tout le monde y voit le même historique.",
  chatNoChannelsLead: "Aucune conversation pour l’instant",
  chatNoMessagesYet: "Aucun message — dites la première chose.",
  chatNoRoom: "Aucune conversation ne correspond.",
  chatNoRoomOpenHint: "Choisissez un canal à gauche, ou créez-en un.",
  chatNoRoomOpenLead: "Choisissez une conversation",
  chatNobodyFound: "Personne ici ne correspond.",
  chatNothingToJoin:
    "Aucun canal public dans cet espace de travail pour l’instant.",
  chatOlder: "Afficher les messages précédents",
  chatOpenFile: "Ouvrir dans Drive",
  chatOwner: "propriétaire",
  chatPeopleFailed: "Impossible d’effectuer cette recherche.",
  chatPeopleHere: "Personnes",
  chatProposalNotYours:
    "Seule la personne qui a demandé peut approuver — cela s’exécuterait avec ses accès.",
  chatQuoteAction: "Citer",
  chatReactFailed: "Impossible d’enregistrer cette réaction.",
  chatRename: "Renommer le canal",
  chatRenameFailed: "Impossible de renommer ce canal.",
  chatRenamePrompt: "Tout le monde dans le canal voit le nouveau nom.",
  chatRenameSave: "Renommer",
  chatAddDescription: "Ajouter une description",
  chatEditDescription: "Modifier la description",
  chatDescriptionPrompt: "Aidez les membres à comprendre l’objectif de ce canal.",
  chatDescriptionSave: "Enregistrer la description",
  chatDescriptionFailed: "Impossible d’enregistrer la description du canal.",
  chatReplyInThread: "Répondre ici",
  chatReplyHere: "Répondre ici",
  chatReplyPrivately: "Répondre en privé",
  chatReplyingHere: "Réponse ici",
  chatReplyingPrivately: (who: string): string => `Réponse privée à ${who}`,
  chatCancelReply: "Annuler la réponse",
  chatSearchClear: "Effacer la recherche",
  chatSearchFailed: "Impossible d’effectuer cette recherche.",
  chatSearchNothing: "Aucun résultat.",
  chatSearchPlaceholder: "Rechercher messages, personnes, canaux…",
  chatSectionArchived: "Archivés",
  chatSectionChannels: "Canaux",
  chatFilterAll: "Tout",
  chatFilterUnread: "Non lus",
  chatFilterThreads: "Fils",
  chatFilterMentions: "Mentions",
  chatCompose: "Composer",
  chatSectionDirect: "Messages directs",
  chatSend: "Envoyer",
  chatSendFailed:
    "Impossible d’envoyer ce message — votre texte est toujours là.",
  chatShare: "Partager quelque chose",
  chatShareAsk: "Demander à alo",
  chatShareAskHint: "Des réponses venues de tout votre espace de travail",
  chatShareFile: "Fichier depuis Drive",
  chatShareFileHint: "Un lien, pas une copie — il reste dans Drive",
  chatShareMention: "Mentionner quelqu’un",
  chatShareMentionHint: "Personnes et agents de cette conversation",
  chatStop: "Arrêter",
  chatThread: "Fil",
  chatThreadClose: "Fermer le fil",
  chatThreadEmpty: "Aucune réponse — lancez-la.",
  chatThreadFailed: "Impossible de charger ce fil.",
  chatThreadPlaceholder: "Répondre…",
  chatToday: "Aujourd’hui",
  chatWhoIsHere: "Qui est là",
  chatWithdrawAction: "Retirer",
  chatWithdrawFailed: "Impossible de retirer ce message.",
  chatWithdrawn: "Ce message a été retiré.",
  chatMessageSent: "Envoyé",
  chatMessageReadBy: (count: number) => `Lu par ${count}`,
  chatYesterday: "Hier",
  docSaving: "Enregistrement…",
  chatAgentAdd: (handle: string): string =>
    `Ajouter @${handle} à cette conversation`,
  chatAgentRemove: (handle: string): string => `Retirer @${handle}`,
  chatArchiveTitle: (name: string): string => `Archiver ${name} ?`,
  chatBeginning: (name: string): string => `C’est le début de ${name}`,
  chatChannelActions: (name: string): string => `Actions pour ${name}`,
  chatComposerPlaceholder: (room: string): string => `Message à ${room}`,
  chatThinking: (handle: string): string => `@${handle} réfléchit`,
  chatUnstage: (name: string): string => `Retirer ${name}`,
  chatReplies: (count: number): string =>
    count === 1 ? "1 réponse" : `${count} réponses`,
  chatMentionsYou: (count: number): string =>
    count === 1
      ? "1 message vous mentionne"
      : `${count} messages vous mentionnent`,
  chatProposalSettled: (state: string): string =>
    state === "approved" ? "Approuvé et effectué." : `Statut : ${state}.`,
  chatAgentRecord: (answers: number, actions: number): string => {
    const said = answers === 1 ? "1 réponse" : `${answers} réponses`;
    if (actions === 0) return said;
    return `${said} · ${actions === 1 ? "1 action approuvée" : `${actions} actions approuvées`}`;
  },
  agentActWhoIsOff: "Voir qui est absent",
  agentWhoIsOffNote:
    "Lit la vue des absences de l’équipe que tout le monde voit déjà ici : qui est absent, et quels jours. Elle ne change rien, ne réserve rien et n’avertit personne.",
  agentWhoIsOffAway: "Absent",
  agentWhoIsOffNobody: "Personne",
  agentWhoIsOffFooter:
    "Noms et jours uniquement — un congé approuvé ne dit jamais pourquoi quelqu’un est absent. Une personne non listée peut tout de même être absente pour une raison que ceci ne couvre pas.",
  agentWhoIsOffCount: (count: number): string =>
    count === 1 ? "1 personne" : `${count} personnes`,
  agentWhoIsOffDays: (count: number): string =>
    count === 1 ? "1 jour" : `${count} jours`,
  baseAddField: "Ajouter un champ",
  baseAddView: "Ajouter une vue",
  baseBoardNeedsSelect:
    "Ajoutez une vue tableau groupée par un champ Sélection pour utiliser ceci.",
  baseByDate: "Par date…",
  baseCalendarNeedsDate:
    "Ajoutez une vue calendrier basée sur un champ Date pour utiliser ceci.",
  baseChoicesPlaceholder: "Choix, séparés par des virgules",
  baseFieldName: "Nom du champ",
  baseGroupBy: "Grouper par…",
  baseLink: "Lien",
  baseLinkNoRecords: "La table liée n’a encore aucun enregistrement.",
  baseLinkNoTable: "Aucune table liée définie.",
  baseLinkTarget: "Table liée…",
  baseNewRow: "Nouvelle ligne",
  baseNewTable: "Nouvelle table",
  baseNoChoices: "Aucun choix pour l’instant — ajoutez-en sur le champ.",
  basePersonPlaceholder: "email@…",
  baseTypeCheckbox: "Case à cocher",
  baseTypeDate: "Date",
  baseTypeLink: "Lien vers une table",
  baseTypeMultiselect: "Sélection multiple",
  baseTypeNumber: "Nombre",
  baseTypePerson: "Personne",
  baseTypeSelect: "Sélection",
  baseTypeText: "Texte",
  baseUncategorised: "Non catégorisé",
  baseUntitledRecord: "Sans titre",
  baseViewBoard: "Tableau",
  baseViewCalendar: "Calendrier",
  baseViewGallery: "Galerie",
  baseViewGrid: "Grille",
  brandEuBadgeDrive: "Vos fichiers, sans verrouillage",
  brandHeadlineDrive: "Vos fichiers.\nVos dossiers.\nVos règles.",
  brandSubtitleDrive:
    "Fichiers, dossiers et documents au même endroit — partagés selon leur emplacement, et toujours à vous.",
  cancel: "Annuler",
  close: "Fermer",
  datePickerClear: "Effacer",
  datePickerToday: "Aujourd’hui",
  homeGoToTasks: "Ouvrir les tâches",
  homeMyTasks: "Mes tâches",
  homeNewTask: "Nouvelle tâche",
  homeNoEventsToday: "Rien dans votre agenda aujourd’hui.",
  homeNoTasks: "Rien à faire. Vous êtes à jour.",
  homeNotifications: "Notifications",
  homeSearchPlaceholder: "Rechercher dans les e-mails, événements, tâches…",
  homeStatTasks: "Tâches à faire aujourd’hui",
  homeSubtitle: "Voici ce qui se passe aujourd’hui.",
  homeToolsTitle: "Vos outils",
  homeToolsSubtitle: "Les applications que vous utilisez le plus, à portée de main.",
  homeTaskOverdue: "En retard",
  homeTaskToday: "Aujourd’hui",
  homeTodaysCalendar: "Agenda du jour",
  homeViewAllTasks: "Voir toutes les tâches",
  homeViewCalendar: "Voir l’agenda",
  homeViewFullCalendar: "Voir l’agenda complet",
  homeViewTasks: "Voir les tâches",
  moduleHr: "Personnes",
  officeUnavailable:
    "Impossible d’ouvrir ce document pour le modifier. Réessayez, ou téléchargez-le.",
  pickerAttach: "Joindre",
  pickerEmpty: "Rien ici pour l’instant.",
  pickerLoadFailed: "Impossible d’ouvrir ce dossier.",
  pickerLoading: "Chargement…",
  pickerMyDrive: "Mon Drive",
  pickerNonePicked: "Aucun fichier choisi",
  pickerPersonalNotice:
    "Les fichiers de Mon Drive n’appartiennent qu’à vous — les personnes de la conversation ne pourront pas les ouvrir. Utilisez un Space pour partager.",
  pickerPlaces: "Où chercher",
  pickerTitle: "Choisir un fichier",
  taskAddAttachment: "Ajouter une pièce jointe",
  taskAddBlocker: "Ajouter un bloqueur",
  taskAddLabel: "Ajouter une étiquette",
  taskAllTasks: "Toutes les tâches",
  taskAssigneeYou: "Vous",
  taskAttachments: "Pièces jointes",
  taskBlockedBy: "Bloquée par",
  taskCalendar: "Calendrier",
  taskCancel: "Annuler",
  taskColAssignee: "Assignée à",
  taskColDue: "Échéance",
  taskColName: "Nom de la tâche",
  taskColPriority: "Priorité",
  taskColProject: "Projet",
  taskColReview: "Revue",
  taskCompactRows: "Lignes compactes",
  taskCreate: "Créer une tâche",
  taskCreateAnother: "Créer une autre tâche",
  taskCreateFirst: "Créez votre première tâche",
  taskCreateLabel: "Créer",
  taskDownload: "Télécharger",
  taskEmptyBody: "Tout est prêt. Commencez par créer votre première tâche.",
  taskEmptyTitle: "Aucune tâche pour l’instant 👋",
  taskFiles: "Fichiers",
  taskFilesEmpty: "Aucun fichier. Joignez-en un depuis n’importe quelle tâche.",
  taskFilter: "Filtrer",
  taskFollow: "Suivre",
  taskFollowers: "Abonnés",
  taskGroup: "Grouper",
  taskGroupAssignee: "Assignée à",
  taskGroupNone: "Aucun",
  taskGroupPriority: "Priorité",
  taskGroupProject: "Projet",
  taskGroupStatus: "Statut",
  taskLabelsTitle: "Étiquettes",
  taskLeave: "Quitter la tâche",
  taskMarkDone: "Marquer comme faite",
  taskMarkNotDone: "Marquer comme non faite",
  taskNamePlaceholder: "p. ex. Concevoir la page d’accueil",
  taskNew: "Nouvelle tâche",
  taskNewLabelPlaceholder: "Nouvelle étiquette…",
  taskNewSubtitle: "Créez une tâche et gardez le fil.",
  taskNewTaskPrompt: "Nom de la nouvelle tâche",
  taskNoBlockerCandidates: "Aucune autre tâche dont dépendre",
  taskOnlyMine: "Seulement mes tâches",
  taskOptions: "Options",
  taskOvByAssignee: "Tâches par personne",
  taskOvCompleted: "Terminées",
  taskOvCompletedLabel: "Terminées",
  taskOvNobody: "Non assignées",
  taskOvProgress: "Progression",
  taskOvTotal: "Total",
  taskOvUpcoming: "Tâches à venir",
  taskOvViewAll: "Tout voir",
  taskOverview: "Vue d’ensemble",
  taskSearchPlaceholder: "Rechercher tâches, projets…",
  taskShowCompleted: "Afficher les terminées",
  taskSort: "Trier",
  taskSortCreated: "Plus récentes",
  taskSortDue: "Échéance",
  taskSortManual: "Manuel",
  taskSortName: "Nom",
  taskSortPriority: "Priorité",
  taskTimeline: "Chronologie",
  taskUnassigned: "Non assignée",
  taskUnscheduled: "Sans échéance",
  taskUploading: "Importation…",
  userAccountantBadge: "Comptable",
  userAccountantHint:
    "Lit les comptes — rapports, approbations de notes de frais et clôture d’une période — et peut consulter factures et affaires sans les modifier. Pas de console d’administration, ni d’accès aux e-mails ou fichiers d’autrui.",
  userAccountantRole: "Comptable",
  userRoles: "Rôles",
  pickerPicked: (count: number, max: number): string =>
    `${count} sur ${max} choisis`,
  taskOvTasksTotal: (n: number) => `${n} tâches au total`,
  hrActions: "Décision",
  hrAddCandidate: "Ajouter un candidat",
  hrAddNote: "Ajouter une note",
  hrAlsoAway: "Déjà absent à ces dates",
  hrApprovalsEmptyBody:
    "Les congés, les notes de frais et les semaines de temps que les gens remettent arrivent ici ensemble, les plus anciens d’abord — pour que personne n’attende parce que sa demande était dans le module que vous avez ouvert en dernier.",
  hrApprovalsEmptyTitle: "Rien n’attend",
  hrApprovalsNoneBody:
    "C’est ici que les congés, les notes de frais et les semaines de temps attendent la personne qui tranche. Vous le verrez lorsque quelqu’un vous sera rattaché, ou lorsque vous tiendrez les comptes.",
  hrApprovalsNoneTitle: "Rien ne vous revient pour décision",
  hrApprovalsTable: "En attente d’une décision",
  hrApprovalsWidgetLabel: "en attente",
  hrApprovalsWidgetTitle:
    "Congés, notes de frais et semaines en attente de votre décision",
  hrApprove: "Approuver",
  hrAskForLeave: "Demander un congé",
  hrAskSubmit: "Demander",
  hrAskSubtitle:
    "Les jours sont déduits du solde du type que vous choisissez, calculés d’après votre propre rythme de travail — vous ne saisissez jamais un nombre de jours.",
  hrAwayControls: "Mois",
  hrAwayCalendar: "Qui est absent, jour par jour",
  hrBalanceBooked: "Réservé",
  hrBalanceLeft: "restants",
  hrBalanceTaken: "Pris",
  hrBalanceThisYear: "Cette année",
  hrBalanceWaiting: "En attente",
  hrCancel: "Annuler",
  hrCancelLeave: "Annuler",
  hrCandidate: "Candidat",
  hrCandidateSubtitle:
    "Ce que disait la candidature. Rien ici n’est lu par une machine — pas de présélection, pas de classement, pas de score.",
  hrClearSearch: "Effacer la recherche",
  hrClose: "Fermer",
  hrCloseOpening: "Clore le tour",
  hrClosedNotice:
    "Ce tour est clos. Son tableau reste lisible et les personnes qui y figurent peuvent encore être déplacées — mais personne de nouveau ne peut y être ajouté.",
  hrContact: "Contact",
  hrCreate: "Créer",
  hrCv: "CV",
  hrCvAttach: "Joindre un CV",
  hrCvDownload: "Télécharger le CV",
  hrCvFailed: "Impossible de télécharger ce fichier.",
  hrCvHint:
    "Classé dans l’espace RH, que seules les RH peuvent ouvrir. Rien ne le lit — pas de présélection, pas de classement, pas de score.",
  hrCvNone: "Aucun CV au dossier.",
  hrCvRemove: "Retirer le CV de ce dossier",
  hrCvReplace: "Remplacer le CV",
  hrCvTrashed: "Le CV qui était au dossier a été mis à la corbeille RH.",
  hrCvUploadFailed:
    "Ce fichier n’a pas été importé, donc rien n’a été enregistré. Réessayez, ou enregistrez les informations sans lui.",
  hrDirectoryControls: "Filtres de l’annuaire",
  hrDirectoryEmptyBody:
    "Dès que les RH auront inscrit la première personne, c’est ici que chacun trouvera ses collègues — qui ils sont, comment les joindre, et à qui ils sont rattachés.",
  hrDirectoryEmptyTitle: "Personne n’est encore dans l’annuaire",
  hrDirectorySearch: "Rechercher des personnes",
  hrDirectoryTable: "Personnes",
  hrDirectoryViews: "Comment lire l’annuaire",
  hrEditCandidate: "Modifier les informations",
  hrEditOpening: "Modifier le poste",
  hrErase: "Effacer ce dossier",
  hrFieldEmail: "E-mail",
  hrFieldEmployment: "Contrat",
  hrFieldFamilyName: "Nom",
  hrFieldFirstDay: "Premier jour d’absence",
  hrFieldGivenName: "Prénom",
  hrFieldJobTitle: "Intitulé du poste",
  hrFieldLastDay: "Dernier jour d’absence",
  hrFieldLocation: "Lieu",
  hrFieldName: "Nom",
  hrFieldPhone: "Téléphone",
  hrFieldRetainUntil: "Conserver jusqu’au",
  hrFieldRole: "Rôle",
  hrFieldSource: "D’où ils viennent",
  hrFieldStartedOn: "Commence le",
  hrFieldTeam: "Équipe",
  hrFieldWorkEmail: "E-mail professionnel",
  hrFigure: "Montant",
  hrHiringControls: "Poste à pourvoir",
  hrHire: "Les ajouter à l’annuaire",
  hrHireEmailHint:
    "Leur adresse professionnelle, si elle est déjà connue. Elle peut être ajoutée plus tard.",
  hrHireNameHint:
    "Séparé à partir du nom figurant sur la candidature. Corrigez si la séparation est mauvaise.",
  hrHireNoAccount:
    "Ceci écrit un dossier dans Personnes. Cela ne crée ni identifiant ni boîte mail — c’est un administrateur qui le fait, et la liste d’intégration comporte une tâche pour cela.",
  hrHireNoKind: "Non précisé",
  hrHireStartHint:
    "Le jour où leurs conditions prennent effet. Tous les soldes de congés sont comptés à partir de là.",
  hrHireSubmit: "Ajouter à l’annuaire",
  hrHireSubtitle:
    "Leur dossier de salarié, et les conditions auxquelles ils commencent. Tout est prérempli depuis la candidature et le poste — corrigez ce qui ne va pas.",
  hrHired: "Ils ont accepté le poste",
  hrHiredExplainer:
    "Déplacer quelqu’un vers Recruté consigne ce qui s’est passé. L’inscrire dans l’annuaire est un acte distinct, effectué ici.",
  hrHolidaysInside: "Un jour férié tombe dans ces dates et n’est pas compté.",
  hrIncludeClosed: "Inclure les tours clos",
  hrIncludeLeavers: "Inclure les personnes parties",
  hrKindApprentice: "Apprentissage",
  hrKindContractor: "Indépendant",
  hrKindFixedTerm: "Durée déterminée",
  hrKindIntern: "Stage",
  hrKindPartTime: "Temps partiel",
  hrKindPermanent: "Durée indéterminée",
  hrLastDayHint: "Le jour de votre retour n’en fait pas partie.",
  hrLeaveApproved: "Réservé",
  hrLeaveCancelled: "Annulé",
  hrLeaveControls: "Filtres des congés",
  hrLeaveDays: "Jours",
  hrLeaveEmptyBody:
    "Demandez ici un jour ou quinze jours. Vous verrez ce que cela coûte à votre solde avant que quiconque ne décide, et qui d’autre est déjà absent ces jours-là.",
  hrLeaveEmptyTitle: "Vous n’avez demandé aucun congé",
  hrLeaveKind: "Type",
  hrLeaveNoneShownBody:
    "Des congés sont enregistrés, mais aucun n’est dans l’état que vous avez demandé.",
  hrLeaveNoneShownTitle: "Rien dans cet état",
  hrLeaveRejected: "Refusé",
  hrLeaveRequested: "En attente",
  hrLeaveShow: "Afficher",
  hrLeaveState: "État",
  hrLeaveTable: "Demandes de congé",
  hrLeaveTeamEmptyBody:
    "Lorsqu’une personne qui vous est rattachée demande des jours de congé, cela arrive ici et dans vos approbations — avec les dates, ce que cela coûte à son solde, et qui d’autre est absent à ce moment-là.",
  hrLeaveTeamEmptyTitle: "Personne n’a demandé de congé",
  hrLeaveWhen: "Quand",
  hrLeaveWhose: "Congé de qui",
  hrLeaveWhy: "Pourquoi",
  hrLeaveWithdrawn: "Retiré",
  hrLeft: "Parti",
  hrLoadFailed: "Impossible de charger cela.",
  hrLocationHint: "Une ville, un bureau, ou « à distance ».",
  hrManager: "Rattaché à",
  hrNewOpening: "Nouveau poste",
  hrNextMonth: "Le mois suivant",
  hrNoMatchBody:
    "Les noms, les postes, les équipes, les adresses e-mail et les numéros de téléphone sont tous fouillés, dans n’importe quel ordre. Essayez avec un mot de moins.",
  hrNoOpeningsBody:
    "Notez le poste pour lequel vous recrutez. Enregistrez les personnes qui postulent au fil de l’eau, et faites-les avancer sur le tableau à mesure que vous les rencontrez.",
  hrNoOpeningsTitle: "Aucun poste encore noté",
  hrNobodyAway: "Personne d’autre n’est absent ces jours-là.",
  hrNobodyAwayBody:
    "Les congés réservés de toute l’entreprise apparaissent ici, pour que vous voyiez qui est absent avant de planifier autour d’eux. Les jours fériés y figurent aussi.",
  hrNotDecided: "Consigné, non décidé",
  hrNotePlaceholder: "Ce qui s’est dit pendant l’entretien…",
  hrNotes: "Notes d’entretien",
  hrNotesEmpty: "Rien n’a encore été noté.",
  hrOneDay: "1 jour",
  hrOpening: "Poste",
  hrOpeningSubtitle:
    "Un poste noté. Publier signifie que le tour est ouvert ; le clore y met fin et fige ce que le poste disait.",
  hrPerson: "Personne",
  hrPolicyRecordedHint:
    "Ce type est consigné plutôt que décidé : il est réservé dès que vous le demandez.",
  hrPreviousMonth: "Le mois précédent",
  hrPublishOpening: "Publier",
  hrQueue: "Type",
  hrQueueExpense: "Note de frais",
  hrQueueLeave: "Congé",
  hrQueueTimesheet: "Semaine",
  hrRangeBackwards: "Le dernier jour est antérieur au premier.",
  hrRetainHint:
    "Six mois après la candidature, sauf indication contraire. Passée cette date, le dossier peut être effacé.",
  hrRetention: "Combien de temps nous gardons ceci",
  hrRetentionExpired: "Date dépassée",
  hrRetentionExplainer:
    "Rien n’est effacé automatiquement. Une fois la date passée, quelqu’un ici décide — et ce qui part, part : les informations, chaque note, et le CV.",
  hrSave: "Enregistrer",
  hrSaveFailed: "Cette modification n’a pas été enregistrée.",
  hrScopeEveryone: "Tout le monde",
  hrScopeMine: "Les miens",
  hrScopeTeam: "Mon équipe",
  hrSendBack: "Renvoyer",
  hrSendBackPlaceholder: "Ce qui doit être corrigé",
  hrSendBackTitle: "Renvoyer ceci ?",
  hrShowBooked: "Réservés",
  hrShowEverything: "Tout",
  hrShowInChart: "Où ils se situent",
  hrShowWaiting: "En attente d’une décision",
  hrSince: "Ici depuis",
  hrSourceHint:
    "Un site d’emploi, une recommandation, une agence — quel que soit le chemin par lequel la candidature vous est parvenue.",
  hrStage: "Étape",
  hrStageApplied: "Candidature",
  hrStageHired: "Recruté",
  hrStageInterview: "Entretien",
  hrStageOffer: "Offre",
  hrStageRejected: "Non retenu",
  hrStageReviewing: "En examen",
  hrStageWithdrawn: "Retiré",
  hrStatusClosed: "Clos",
  hrStatusDraft: "Brouillon",
  hrStatusOpen: "Ouvert",
  hrTabApprovals: "Approbations",
  hrTabAway: "Qui est absent",
  hrTabDirectory: "Annuaire",
  hrTabHiring: "Recrutement",
  hrTabTemplates: "Modèles de lettre",
  hrTemplatesTitle: "Modèles de lettre",
  hrTemplatesIntro: "Rédigez une fois le texte approuvé, puis laissez les RH créer un brouillon personnel sans le ressaisir.",
  hrTemplatesLoadFailed: "Les modèles de lettre n’ont pas pu être chargés.",
  hrTemplatesEmpty: "Aucun modèle de lettre",
  hrTemplatesEmptyBody: "Créez le texte que votre entreprise accepte d’envoyer. Rien n’est envoyé depuis cet écran.",
  hrTemplateNew: "Nouveau modèle",
  hrTemplateCreateTitle: "Créer un modèle de lettre",
  hrTemplateEditTitle: "Modifier le modèle de lettre",
  hrTemplateEditorIntro: "Les champs ne sont remplis que lorsque les RH créent un brouillon pour une personne précise.",
  hrTemplateName: "Nom du modèle",
  hrTemplateSubject: "Objet de l’e-mail",
  hrTemplateBody: "Texte de la lettre",
  hrTemplateBodyHint: "Utilisez les champs approuvés ci-dessous. Les champs inconnus sont refusés.",
  hrTemplateInsertField: "Insérer un champ",
  hrTemplateSave: "Enregistrer le modèle",
  hrTemplateSaveFailed: "Le modèle de lettre n’a pas été enregistré.",
  hrTemplateDelete: "Supprimer le modèle",
  hrTemplateDeleteTitle: (name: string) => `Supprimer ${name} ?`,
  hrTemplateDeleteBody: "Les brouillons existants restent inchangés. Ce modèle ne sera plus proposé pour les nouvelles lettres.",
  hrTemplateDeleteFailed: "Le modèle de lettre n’a pas été supprimé.",
  hrTemplateFields: (count: number) => count === 1 ? "1 champ" : `${count} champs`,
  hrTabLeave: "Mes congés",
  hrThisMonth: "Ce mois-ci",
  hrUnpaid: "Sans solde",
  hrViewOrg: "Organigramme",
  hrViewPeople: "Personnes",
  hrWaitingSince: "Remis le",
  hrWhat: "En attente de vous",
  hrWhyHint:
    "Facultatif. Seule la personne qui tranche le lit, et ce n’est jamais journalisé.",
  hrWithdraw: "Le retirer",
  hrYou: "Vous",
  hrAppliedOn: (moment: string) => `Candidature ${moment}`,
  hrApprovalsQueueFailed: (kinds: string) =>
    `Une partie de ce qui attend n’a pas pu être lue (${kinds}), cette liste est donc incomplète. Tout le reste est affiché.`,
  hrAwayThisMonth: (count: number) =>
    count === 1
      ? "1 personne absente ce mois-ci"
      : `${count} personnes absentes ce mois-ci`,
  hrBalanceAsOf: (day: string) =>
    `Calculé le ${day}, d’après votre propre rythme de travail.`,
  hrCloseConfirm: (title: string) =>
    `Clore le tour pour ${title} ? Les personnes qui ont postulé restent comme trace de ce qui s’est passé, et le tour ne peut pas être rouvert.`,
  hrClosedOn: (day: string) => `clos le ${day}`,
  hrCountOf: (kind: string, count: number) => `${kind} : ${count}`,
  hrCvOnFile: (fileName: string) =>
    fileName === ""
      ? "Un CV est au dossier. Choisir un fichier le remplace ; celui qu’il remplace part à la corbeille RH."
      : `${fileName} est au dossier. Choisir un fichier le remplace ; celui qu’il remplace part à la corbeille RH.`,
  hrDayAway: (day: string, count: number) =>
    count === 0 ? `${day} : personne d’absent` : `${day} : ${count} absents`,
  hrDaysOf: (days: string) => `${days} jours`,
  hrEraseConfirm: (name: string) =>
    `Effacer tout ce qui concerne ${name} ? Ses informations, chaque note écrite à son sujet et son CV sont supprimés définitivement. Cette action est irréversible.`,
  hrFactOf: (label: string, value: string) => `${label} ${value}`,
  hrHireKnown: (name: string) =>
    `${name} figure déjà dans l’annuaire avec cette adresse. Ajouter ce dossier créerait un second collègue avec le même e-mail.`,
  hrHireKnownLeft: (name: string) =>
    `${name} avait cette adresse et est parti. S’il s’agit de la même personne qui revient, l’ajouter ici est correct — son ancien dossier reste tel quel.`,
  hrLeaveBetween: (from: string, to: string) => `${from} – ${to}`,
  hrLeaveOf: (policy: string, from: string, to: string) =>
    from === to ? `${policy}, ${from}` : `${policy}, ${from} – ${to}`,
  hrMoreAway: (count: number) => `+${count} de plus`,
  hrNoMatchTitle: (query: string) => `Personne ne correspond à « ${query} »`,
  hrNobodyAwayTitle: (month: string) => `Personne n’est absent en ${month}`,
  hrOpenedOn: (day: string) => `ouvert depuis le ${day}`,
  hrPeopleCount: (count: number) =>
    count === 1 ? "1 personne" : `${count} personnes`,
  hrReportsCount: (count: number) =>
    count === 1 ? "1 rattaché" : `${count} rattachés`,
  hrRetentionUntil: (day: string) => `Conservé jusqu’au ${day}.`,
  hrSendBackBody: (person: string) =>
    `${person} le reverra, modifiable, avec ce que vous écrivez ici. Dites ce qui doit être corrigé.`,
  hrShowingOf: (shown: number, total: number) => `${shown} sur ${total}`,
  hrWaitingCount: (count: number) =>
    count === 1 ? "1 en attente" : `${count} en attente`,
  hrWorkingDays: (days: number) => (days === 1 ? "1 jour" : `${days} jours`),
  userApps: "Applications",
  userAppsHint: "Seules les applications cochées apparaissent dans la navigation de cette personne, et le serveur refuse les autres — ceci ne masque pas, ceci ferme. Le courrier et l’accueil ne peuvent pas être désactivés. Cocher une application ne donne pas accès à tout ce qu’elle contient : Finance demande toujours le rôle de comptable, et un Space toujours d’en être membre.",
  userAppsSelfHint: "Ceci est votre propre compte. Un administrateur n’est jamais exclu, donc ces interrupteurs ne changent rien à ce que vous pouvez ouvrir — ils sont conservés au cas où ce compte cesserait un jour d’être administrateur.",
  accessModuleOff: "Cette application est désactivée pour votre compte.",
  accessModuleOffHint: "Un administrateur de l’espace de travail peut la réactiver.",
  accessBackHome: "Retour à l’accueil",
  userInvite: "Créer une invitation",
  userInviteReady: "Lien d’installation",
  userInviteCopy: "Copier",
  userInviteCopied: "Copié",
  userInviteHint: "Envoyez ce lien à votre collègue. Il fonctionne une seule fois, expire après sept jours, et c’est la personne qui choisit son mot de passe et son adresse de récupération — vous ne les connaîtrez jamais. Ce lien n’est affiché qu’une fois.",
  inviteTitle: "Configurez votre compte",
  inviteUnavailable: "Cette invitation ne fonctionne plus",
  inviteAskAdmin: "Demandez-en une nouvelle à l’administrateur de votre espace de travail.",
  inviteLoadFailed: "Cette invitation a expiré ou a déjà été utilisée.",
  inviteFailed: "Impossible d’enregistrer. Réessayez.",
  invitePassword: "Choisissez un mot de passe",
  invitePasswordHint: "Au moins 8 caractères. Vous seul le connaissez.",
  inviteRecovery: "Adresse de récupération",
  inviteRecoveryPlaceholder: "vous@ailleurs.fr",
  inviteRecoveryHint: "Une adresse que vous pouvez lire ailleurs — pas cette nouvelle. Si vous oubliez un jour votre mot de passe, c’est le seul moyen de revenir sans le demander à un administrateur.",
  inviteSubmit: "Configurer le compte",
  inviteWorking: "Configuration…",
  inviteDoneTitle: "C’est fait",
  inviteGoToSignIn: "Aller à la connexion",
  inviteFor: (email: string): string => `Pour ${email}`,
  inviteDoneBody: (email: string): string =>
    `Vous pouvez maintenant vous connecter en tant que ${email} avec le mot de passe que vous venez de choisir.`,

  // Les adresses auxquelles un site répond (S2.15c3). Chaque prix est dit
  // deux fois — ce qu’il coûte aujourd’hui et chaque année ensuite — parce
  // que le renouvellement est la moitié que cache un prix d’appel.
  sitesDomains: "Domaines",
  sitesDomainsLoading: "Chargement des domaines…",
  sitesDomainsLoadFailed:
    "Les domaines de ce site n’ont pas pu être chargés. Vérifiez votre connexion et réessayez.",
  sitesDomainAloAddress: "Ce site reste toujours accessible à l’adresse",
  sitesDomainOwned: "Un domaine que vous possédez déjà",
  sitesDomainOwnedHint:
    "Ajoutez le domaine, publiez l’enregistrement affiché chez votre hébergeur DNS, puis appuyez sur Vérifier. Rien ne change pour vos visiteurs tant qu’il n’est pas vérifié.",
  sitesDomainAddress: "Domaine",
  sitesDomainPlaceholder: "exemple.com",
  sitesDomainAdd: "Ajouter le domaine",
  sitesDomainAddFailed: "Ce domaine n’a pas pu être ajouté.",
  sitesDomainNoneBody:
    "Aucun domaine personnel n’est encore connecté. Ajoutez-en un que vous possédez déjà, ou achetez-en un ci-dessous, et ce site répondra aussi à cette adresse.",
  sitesDomainStatusPending: "En attente de l’enregistrement",
  sitesDomainStatusVerified: "Vérifié",
  sitesDomainStatusLive: "En service",
  sitesDomainCheck: "Vérifier",
  sitesDomainVerifyFailed: "Le domaine n’a pas pu être vérifié.",
  sitesDomainNotYet:
    "L’enregistrement n’est pas encore visible. Les modifications DNS mettent quelques minutes à se propager : laissez l’enregistrement en place et vérifiez à nouveau dans un instant.",
  sitesDomainVerifiedNow: (domain: string): string =>
    `${domain} est vérifié. Ce site répond désormais à cette adresse.`,
  sitesDomainRecordTitle: "Publiez cet enregistrement chez votre hébergeur DNS",
  sitesDomainRecordName: "Nom",
  sitesDomainRecordType: "Type",
  sitesDomainRecordValue: "Valeur",
  sitesDomainRecordHint:
    "Laissez l’enregistrement en place jusqu’à ce que la vérification aboutisse. Certains hébergeurs DNS ajoutent eux-mêmes le domaine au nom : si c’est le cas du vôtre, ne l’indiquez pas.",
  sitesDomainPointHint: (host: string): string =>
    `Dernière étape chez votre hébergeur DNS : faites pointer le domaine vers ${host} avec un CNAME. Un domaine racine demande l’enregistrement ALIAS ou ANAME de votre hébergeur.`,
  sitesDomainCopy: "Copier",
  sitesDomainCopied: "Copié",
  sitesDomainRemove: "Retirer",
  sitesDomainRemoveConfirm: "Oui, le retirer",
  sitesDomainRemoveHint:
    "alo cesse de répondre à ce domaine. Le domaine lui-même reste le vôtre : rien n’est abandonné auprès du registre.",
  sitesDomainRemoveFailed: "Ce domaine n’a pas pu être retiré.",
  sitesDomainBuy: "Acheter un domaine",
  sitesDomainBuyHint:
    "Cherchez un nom. Vous voyez ce qu’il coûte cette année et chaque année suivante avant tout achat.",
  sitesDomainSearchLabel: "Le nom que vous souhaitez",
  sitesDomainSearchPlaceholder: "acme",
  sitesDomainSearching: "Recherche…",
  sitesDomainSearchInvite:
    "Saisissez un nom pour voir quelles extensions sont libres.",
  sitesDomainSearchFailed: "Ce nom n’a pas pu être vérifié.",
  sitesDomainCatalogFailed: "Les tarifs des domaines n’ont pas pu être chargés.",
  sitesDomainUnconfiguredTitle: "L’achat de domaines n’est pas activé ici",
  sitesDomainUnconfiguredBody:
    "Cet espace de travail ne peut pas enregistrer de noms de domaine. Vous pouvez toujours connecter un domaine que vous possédez déjà.",
  sitesDomainNotBuyable:
    "Cet espace de travail peut afficher les tarifs mais ne peut pas encore enregistrer de domaine, faute de serveurs de noms configurés.",
  sitesDomainTestRegistrar: (name: string): string =>
    `${name} est un bureau d’enregistrement de test : rien n’est facturé et aucun nom réel n’est enregistré.`,
  sitesDomainRegistrarLine: (name: string, country: string): string =>
    `Les domaines sont enregistrés via ${name} (${country}). Tarifs hors TVA.`,
  sitesDomainAvailable: "Libre",
  sitesDomainTaken: "Déjà enregistré",
  sitesDomainBlocked: "Non commercialisé",
  sitesDomainUnsupportedEnding: "alo ne vend pas cette extension",
  sitesDomainPremium: "Nom premium",
  sitesDomainPremiumHint:
    "Le registre facture ce nom au-dessus du tarif habituel de son extension. Son tarif de renouvellement est celui affiché, et non le tarif ordinaire.",
  sitesDomainPriceLine: (today: string, renewal: string): string =>
    `${today} aujourd’hui, puis ${renewal} par an`,
  sitesDomainChoose: "Acheter ce domaine",
  sitesDomainPurchaseTitle: (domain: string): string => `Acheter ${domain}`,
  sitesDomainPurchaseSubtitle:
    "À qui le domaine est enregistré, et pour combien de temps. Vous approuvez le prix à l’étape suivante ; rien n’est facturé avant cela.",
  sitesDomainYears: "Payé pour",
  sitesDomainYearsHint:
    "Le nombre d’années couvertes par le premier paiement. Ensuite, c’est une année à la fois.",
  sitesDomainYearsOption: (years: number): string =>
    years === 1 ? "1 an" : `${years} ans`,
  sitesDomainAutoRenew: "Renouveler ce domaine automatiquement",
  sitesDomainAutoRenewHint:
    "Un domaine qui n’est pas renouvelé est perdu, et n’importe qui peut alors le prendre. Ne désactivez ceci que si vous comptez le renouveler vous-même.",
  sitesDomainAutoRenewOn: "Il se renouvelle automatiquement chaque année.",
  sitesDomainAutoRenewOff:
    "Il ne se renouvelle pas automatiquement : vous devez le renouveler vous-même avant son expiration, sans quoi vous le perdez.",
  sitesDomainRegistrant: "Enregistré au nom de",
  sitesDomainRegistrantHint:
    "Le registre exige une personne ou une société réelle et joignable. Ces informations vont au registre : elles ne sont jamais affichées sur votre site.",
  sitesDomainRegistrantName: "Nom complet",
  sitesDomainRegistrantOrganisation: "Société (laissez vide s’il n’y en a pas)",
  sitesDomainRegistrantEmail: "Adresse e-mail",
  sitesDomainRegistrantEmailHint:
    "Le registre écrit ici au sujet de l’expiration et de la vérification. Une adresse que personne ne lit fait perdre le domaine.",
  sitesDomainRegistrantStreet: "Rue et numéro",
  sitesDomainRegistrantPostalCode: "Code postal",
  sitesDomainRegistrantCity: "Ville",
  sitesDomainRegistrantCountry: "Pays",
  sitesDomainRegistrantCountryHint:
    "Le code pays à deux lettres, par exemple fr ou be.",
  sitesDomainRegistrantPhone: "Téléphone",
  sitesDomainRegistrantPhoneHint:
    "Sous forme internationale, par exemple +33123456789.",
  sitesDomainRequirementEea:
    "Cette extension n’est vendue qu’à un titulaire établi dans l’Espace économique européen.",
  sitesDomainRequirementCountry: (country: string): string =>
    `Cette extension n’est vendue qu’à un titulaire établi dans le pays ${country}.`,
  sitesDomainSeePrice: "Voir le prix",
  sitesDomainQuoteFailed: "Le prix de ce domaine n’a pas pu être établi.",
  sitesDomainApproveTitle: "Approuver ce prix",
  sitesDomainApproveSubtitle: (domain: string): string =>
    `Ce que coûte ${domain}, en totalité, avant toute facturation.`,
  sitesDomainQuoteName: "Domaine",
  sitesDomainQuoteTerm: "Payé pour",
  sitesDomainQuoteToday: "Aujourd’hui",
  sitesDomainQuoteRenewal: "Chaque année suivante",
  sitesDomainApproveAction: (price: string): string => `Approuver ${price}`,
  sitesDomainApproveHint:
    "Approuver consigne votre accord sur ces montants exacts. Si le prix change avant le paiement, alo vous redemande votre accord au lieu de facturer un autre montant.",
  sitesDomainApproveFailed: "Ce prix n’a pas pu être approuvé.",
  sitesDomainPurchases: "Domaines achetés ici",
  sitesDomainPurchasesHint:
    "Tous les domaines dont l’achat a été entamé pour ce site, et où chacun en est.",
  sitesDomainPurchasesNone:
    "Aucun domaine n’a encore été acheté pour ce site.",
  sitesDomainPurchasesLoadFailed:
    "Les achats de domaines n’ont pas pu être chargés.",
  sitesDomainRefresh: "Actualiser",
  sitesDomainTermPrice: (price: string, years: number): string =>
    years === 1
      ? `${price} pour la première année`
      : `${price} pour les ${years} premières années`,
  sitesDomainRenewalLine: (price: string): string => `puis ${price} par an`,
  sitesDomainApprovedOn: (when: string): string => `Prix approuvé le ${when}.`,
  sitesDomainAttempts: (attempts: number): string =>
    `Tentative d’enregistrement ${attempts} ; alo continue d’essayer.`,
  sitesDomainCancel: "Annuler l’achat",
  sitesDomainCancelConfirm: "Oui, annuler l’achat",
  sitesDomainCancelFailed: "Cet achat n’a pas pu être annulé.",
  sitesDomainStateQuoted: "En attente de votre approbation",
  sitesDomainStateApproved: "Approuvé",
  sitesDomainStateAwaitingPayment: "En attente de paiement",
  sitesDomainStatePaid: "Payé",
  sitesDomainStateRegistering: "Enregistrement en cours",
  sitesDomainStateRegistered: "Enregistré",
  sitesDomainStateConfigured: "En service",
  sitesDomainStateFailed: "Non abouti",
  sitesDomainStateCancelled: "Annulé",
  sitesDomainStepQuoted:
    "Rien n’a été facturé. Approuvez le prix et l’achat passe au paiement.",
  sitesDomainStepApproved:
    "Vous avez approuvé ce prix. Le paiement vient ensuite : dès qu’il est encaissé, alo enregistre le domaine et le rattache à ce site de lui-même.",
  sitesDomainStepAwaitingPayment:
    "En attente de l’encaissement du paiement. L’enregistrement démarre de lui-même dès qu’il arrive.",
  sitesDomainStepPaid: "Payé. L’enregistrement démarre dans la minute.",
  sitesDomainStepRegistering:
    "Le bureau d’enregistrement enregistre le nom en ce moment.",
  sitesDomainStepRegistered: (domain: string): string =>
    `${domain} est enregistré à votre nom. Rattachement à ce site en cours.`,
  sitesDomainStepConfigured: (domain: string): string =>
    `${domain} est enregistré et dessert ce site.`,
  sitesDomainStepFailed:
    "Cet achat n’a pas pu aboutir. Rien de plus ne sera facturé à ce titre.",
  sitesDomainStepCancelled: "Annulé. Rien n’a été facturé.",
  sitesDomainOwnerOnly:
    "Seul le propriétaire de ce site peut acheter ou gérer ses noms de domaine. Vous pouvez toujours modifier et publier le site lui-même.",

  // alo Campagnes (ADR 0044, vague C1) — l’écran de l’audience.
  moduleCampaigns: "Campagnes",
  campaignsTitle: "Audience",
  campaignsSubtitle:
    "Toutes les personnes que cet espace de travail peut atteindre, et celles qu’il ne peut pas — avec la raison.",
  campaignsCountriesLabel: "Pays",
  campaignsCountriesHint:
    "Codes à deux lettres, séparés par des virgules. Vide signifie partout.",
  campaignsCountriesPlaceholder: "BE, NL",
  campaignsPurchaseLabel: "Achats",
  campaignsPurchaseAny: "Tout le monde",
  campaignsPurchaseBought: "A acheté",
  campaignsPurchaseNotBought: "N’a pas acheté",
  campaignsPeriodLabel: "Au cours des",
  campaignsPeriodEver: "Depuis toujours",
  campaignsPeriodDays: (days: number): string => `${days} derniers jours`,
  campaignsEveryone: "Tout le monde",
  campaignsSegmentsLabel: "Questions enregistrées",
  campaignsSaveSegment: "Enregistrer cette question",
  campaignsSegmentNamePrompt: "Quel nom donner à cette question ?",
  campaignsSegmentNamePlaceholder: "Clients belges",
  campaignsDeleteSegment: "Supprimer",
  campaignsDeleteSegmentConfirm: (name: string): string =>
    `Supprimer la question « ${name} » ? Aucun consentement ni aucune désinscription n’est touché — seule la question disparaît.`,
  campaignsTallyMailable: (mailable: number, matched: number): string =>
    `${mailable} personnes sur ${matched} recevront le message`,
  campaignsTallyNobody:
    "Personne dans cet espace de travail ne correspond à cette question.",
  campaignsExcludedCount: (people: number, reason: string): string =>
    `${people} · ${reason}`,
  campaignsWillBeMailed: "Recevra le message",
  campaignsReasonNoConsent: "N’a jamais donné son accord",
  campaignsReasonUnsubscribe: "Désinscrit",
  campaignsReasonHardBounce: "Message non distribué",
  campaignsReasonComplaint: "Nous a signalés comme indésirables",
  campaignsReasonManual: "Nous a demandé d’arrêter",
  campaignsTableLabel: "Personnes sélectionnées par cette question",
  campaignsColPerson: "Personne",
  campaignsColCountry: "Pays",
  campaignsColKnownFrom: "Connue par",
  campaignsColStatus: "Statut",
  campaignsSourceBillingCustomer: "Client",
  campaignsSourceCrmDeal: "Affaire",
  campaignsSourceSiteForm: "Formulaire du site",
  campaignsNoMatches: "Personne ne correspond à cette question.",
  campaignsMore: "Afficher plus de personnes",
  campaignsLoadFailed: "L’audience n’a pas pu être lue.",
  campaignsSegmentsFailed:
    "Vos questions enregistrées n’ont pas pu être lues.",
  campaignsSaveFailed: "Cette question n’a pas pu être enregistrée.",
  campaignsDeleteFailed: "Cette question n’a pas pu être supprimée.",
  campaignsEmptyTitle: "Personne à contacter pour l’instant",
  campaignsEmptyBody:
    "Les personnes apparaissent ici dès que cet espace de travail a un client, une affaire avec une adresse e-mail, ou quelqu’un qui a rempli un formulaire sur son site. Les carnets d’adresses personnels ne sont jamais utilisés.",
  campaignsNothingSentYet:
    "Rien n’est envoyé depuis cet écran. L’envoi de campagnes exige sa propre adresse, distincte de votre courrier quotidien, afin qu’une lettre d’information ne puisse jamais compromettre la remise de vos factures.",

  // La lettre telle qu’une personne la recevra réellement (vague C3.6). Les
  // mots qui comptent le plus ici sont l’avertissement et les libellés
  // "Afficher comme" : un aperçu est l’avis de notre moteur de rendu, et la
  // copie que personne ne relit est celle qui part vers tous ceux dont aucun
  // nom n’est enregistré.
  campaignsViewsLabel: "Que regarder",
  campaignsTabAudience: "Audience",
  campaignsTabLetters: "Lettres",
  campaignsLettersTitle: "Lettres",
  campaignsLettersSubtitle: "Chaque lettre telle qu’une personne la recevra réellement.",
  campaignsLetterLabel: "Lettre",
  campaignsNoLettersTitle: "Aucune lettre pour l’instant",
  campaignsNoLettersBody:
    "Une lettre s’écrit dans le même éditeur qu’un document : titres, paragraphes, tableaux et code. Dès qu’il en existe une, elle apparaît ici, rendue exactement telle qu’elle arrivera.",
  campaignsShowAsLabel: "Afficher comme",
  campaignsShowAsHint: "Les deux sont réelles. La moitié d’une audience n’a aucun nom enregistré.",
  campaignsShowAsRecipient: "Quelqu’un que vous pouvez contacter",
  campaignsShowAsFallbacks: "Quelqu’un dont vous ne savez rien",
  campaignsPartLabel: "Partie",
  campaignsPartHint:
    "Chaque lettre contient les deux. Certaines personnes, et tous les filtres, lisent la version simple.",
  campaignsPartHtml: "Mise en forme",
  campaignsPartText: "Texte simple",
  campaignsPreviewFrameLabel: "La lettre telle qu’elle sera reçue",
  campaignsPreviewSubject: "Objet",
  campaignsPreviewPreheader: "Texte d’aperçu",
  campaignsPreviewNoPreheader:
    "Aucun — les logiciels de messagerie afficheront la première ligne de la lettre à la place.",
  campaignsAgainstRecipient: (person: string) => `Voici la copie que reçoit ${person}.`,
  campaignsAgainstFallbacks:
    "Voici la copie que reçoit toute personne dont vous ne savez rien — chaque valeur personnalisée ci-dessous est votre propre formulation de repli.",
  campaignsAgainstNobodyYet:
    "Il n’y a encore personne à contacter : voici donc la copie que reçoit une personne dont vous ne savez rien. Chaque valeur personnalisée ci-dessous est votre propre formulation de repli.",
  campaignsPreviewCaveat:
    "Ceci est l’avis de notre moteur de rendu, pas une preuve. Sous Windows, Outlook dessine le courrier avec le moteur de Word et chaque logiciel diffère — placez une copie de test dans vos brouillons et lisez-la là où vos destinataires la liront.",
  campaignsTestDraft: "Mettre une copie de test dans mes brouillons",
  campaignsTestDraftDone: (address: string) =>
    `Une copie se trouve dans vos brouillons, adressée à ${address}. Rien n’a été envoyé — ouvrez-la dans votre messagerie, ou envoyez-la-vous pour voir comment un vrai logiciel la dessine.`,
  campaignsTestDraftFailed: "Cette copie de test n’a pas pu être écrite.",
  campaignsFieldsTitle: "Ce que sont devenues les valeurs personnalisées",
  campaignsColField: "Valeur",
  campaignsColPrinted: "Se lit",
  campaignsColWhoseWords: "Mots de qui",
  campaignsFieldTheirs: "De leur fiche",
  campaignsFieldFallback: "Votre repli",
  campaignsNoFields: "Cette lettre dit la même chose à tout le monde.",
  campaignsFieldFirstName: "Prénom",
  campaignsFieldName: "Nom complet",
  campaignsFieldEmail: "Adresse e-mail",
  campaignsFieldCountry: "Pays",
  campaignsVocabularyTitle: "Ce que vous pouvez personnaliser",
  campaignsFieldExample: (field: string) => `{{${field}|vos mots}}`,
  campaignsVocabularyHint:
    "Les mots après la barre sont ce que lit une personne dont vous ne savez rien. Ils ne sont pas facultatifs : une valeur sans repli, c’est de là que vient « Bonjour , ».",
  campaignsLettersFailed: "Vos lettres n’ont pas pu être lues.",
  campaignsPreviewFailed: "Cette lettre n’a pas pu être rendue.",

  // La page au bout d’un lien de désabonnement — le seul écran de ce produit
  // qu’un inconnu lit, et il y arrive déjà agacé. Chaque phrase est simple,
  // courte et dit exactement ce qu’une pression a fait.
  campaignUnsubscribeLoading: "Vérification de ce lien…",
  campaignUnsubscribeTitle: "Arrêter ces e-mails",
  campaignUnsubscribeSubtitle: (topic: string) =>
    `Ce message a été envoyé en tant que « ${topic} ». Vous pouvez arrêter ce type seul, ou tout arrêter.`,
  campaignUnsubscribeSubtitleUntopiced:
    "Vous pouvez cesser de recevoir des e-mails de cet espace de travail. Une seule pression suffit.",
  campaignUnsubscribeStopTopic: (topic: string) =>
    `Ne plus m’envoyer « ${topic} »`,
  campaignUnsubscribeStopAll: "Ne plus rien m’envoyer",
  campaignUnsubscribeAlreadyStopped:
    "Cet espace de travail a déjà reçu la consigne de ne plus vous écrire. Vous n’avez rien d’autre à faire.",
  campaignUnsubscribeAlreadyDeclined: (topic: string) =>
    `Vous avez déjà arrêté « ${topic} ». Vous pouvez encore tout arrêter ci-dessous.`,
  campaignUnsubscribeDoneTitle: "C’est fait",
  campaignUnsubscribeDoneAll:
    "Cet espace de travail ne vous écrira plus. Rien d’autre n’est nécessaire.",
  campaignUnsubscribeDoneTopic: (topic: string) =>
    `« ${topic} » ne vous sera plus envoyé.`,
  campaignUnsubscribeDoneTopicNote:
    "Les autres types d’e-mails de cet espace de travail — les factures et les réponses, par exemple — continueront de vous parvenir. Revenez sur ce lien pour les arrêter aussi.",
  campaignUnsubscribeFinalNote:
    "Cela ne peut pas être annulé depuis cette page. Si vous changez d’avis, demandez-le directement à l’expéditeur.",
  campaignUnsubscribeNoAccountNote:
    "Aucun compte et aucune connexion ne sont nécessaires. Cette page ne concerne que l’adresse à laquelle ce message a été envoyé.",
  campaignUnsubscribeUnknownTitle: "Ce lien ne fonctionne plus",
  campaignUnsubscribeUnknownLink:
    "Nous ne reconnaissons pas ce lien de désabonnement. Si vous l’avez copié depuis un e-mail, ouvrez le lien depuis l’e-mail lui-même — ou répondez à l’expéditeur en lui demandant d’arrêter.",
  campaignUnsubscribeFailed:
    "Cela n’a pas pu être enregistré pour l’instant. Appuyez de nouveau sur le bouton.",
};
