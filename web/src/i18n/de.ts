// German (Deutsch) catalog. Typed as `Partial<Catalog>`: any key not
// present here falls back to English, so this grows module by module
// without ever showing a blank label. Conventions: Siezen throughout,
// German quotes „…“ for quoted names, and the product's own type names
// (Space, Base, Sheet, Doc) stay untranslated, as in every language.
//
// Shipped so far (M4.1): the mail daily-driver surface — brand + module
// rail, Home, shell, Contacts, IMAP import, auth (sign-in, two-factor,
// signup, password reset), Agenda incl. sharing, Tasks, Mail (list,
// reading pane, compose, folders, delegation, app passwords, categories,
// Transfer, filters, spam banner, unsubscribe) and Mail settings —
// and (tranche 2) Docs incl. the block editor and technical authoring,
// Drive + Spaces, Sheets, the Office embed, the search overlay and the
// Drive file picker — and (tranche 3) Chat and Meet, incl. the small
// shared words those two surfaces are first to use (add/save/delete,
// approve/discard on agent cards) — and (tranche 4) the admin console
// (overview, domains + DKIM, audit log, security checks, groups & lists,
// users & invitations, app switches, AI providers), the control plane,
// the invitation page, the record-history panel, and the compose
// recipient strays tranche 1's prefix list missed — and (tranche 5) the
// first business cluster: Billing entire (customers, price list,
// invoices, lifecycle, payments, VAT report, quotes, printing identity,
// multi-currency, reminders, recurring), CRM entire (board, deals,
// win/loss, the billing handoff, report, log, linked conversations) and
// Insights entire (boards, the gallery, ask-to-chart, chart labels),
// plus the agent cards those three surfaces render — and (tranche 6)
// Projects entire (the engagement list, the week grid, milestones and
// templates, the approvals inbox, the profitability report, the timer)
// and Finance entire (expense claims, the bank and reconciliation, the
// chart of accounts, the four reports), plus the agent cards both
// render — and (tranche 7) the rest of the assistant's cards (mail,
// calendar, chat and Drive acts), the Drive Base family, Inventory
// entire (catalog, stock, movements, purchase + sales orders, the
// order book, scanning), HR entire (recruiting, letter templates, the
// directory and org chart, leave, the absence month, approvals) and
// Campaigns entire (audience, letters, the unsubscribe page) — and
// (tranches 8 and 9) Sites entire: the builder half, then the commerce
// half (catalog, bookings, tickets, shop, orders, collections, custom
// code, domains). The catalog has been complete since 2026-08-27 and is
// held to parity with en/fr/nl by the drift ratchet in `locale.test.ts`.
// Vocabulary held from tranche 4: a document is "ausgestellt"
// (matching auditActionIssue), a declined quote is "abgelehnt"
// (matching auditActionDecline), a sent-back timesheet is
// "zurückgewiesen" (matching auditActionReject), and the module names
// stay the rail's.
import type { Catalog } from "./en";

export const de: Partial<Catalog> = {
  // brand
  appName: "alo",
  tagline: "Der souveräne, KI-native Arbeitsplatz für Europa.",

  // modules (rail labels + titles)
  moduleHome: "Start",
  moduleMail: "E-Mail",
  moduleAgenda: "Kalender",
  moduleChat: "Chat",
  moduleMeet: "Meet",
  moduleDrive: "Drive",
  moduleDocs: "Dokumente",
  moduleBilling: "Rechnungen",
  moduleTasks: "Aufgaben",
  moduleAi: "KI fragen",
  moduleSearch: "Suche",
  moduleCrm: "Vertrieb",
  crmWorkspaceSubtitle: "Führen Sie Chancen vom ersten Gespräch bis zum sicheren Abschluss.",
  crmFocusEyebrow: "Tagesfokus",
  crmFocusTitle: "Pipeline-Überblick",
  crmFocusHint: "Beginnen Sie mit Chancen, die eine Entscheidung oder Nachverfolgung benötigen.",
  crmFocusOpen: "Offene Deals",
  crmFocusClosingSoon: "Abschluss in 14 Tagen",
  crmFocusOverdue: "Erwarteter Abschluss überschritten",
  crmFocusQuiet: "Seit 14 Tagen ohne Aktivität",
  crmAttentionTitle: "Benötigt Aufmerksamkeit",
  crmAttentionCount: (count: number) => `${count} zu prüfen`,
  crmAttentionOverdue: (day: string) => `Erwartet am ${day}`,
  crmAttentionQuiet: "Keine aktuelle Bewegung",
  crmAttentionOpen: (deal: string) => `${deal} prüfen`,
  moduleSites: "Websites",
  moduleBranding: "Marke",
  moduleInsights: "Auswertungen",
  moduleProjects: "Projekte",
  moduleFinance: "Finanzen",
  financeWorkspacePurpose: "Liquidität, Ausgaben und Konten in einem Finanzarbeitsbereich.",
  financeTabOverview: "Übersicht",
  financeOverviewEyebrow: "Finanzkontrolle",
  financeOverviewTitle: "Ihr Finanzarbeitsbereich",
  financeOverviewSubtitle: "Sehen Sie frühzeitig, was Ihre Aufmerksamkeit braucht.",
  financeOverviewLoadFailed: "Die Finanzübersicht konnte nicht geladen werden.",
  financeAttentionCount: (count: number) => `${count} ${count === 1 ? "Vorgang braucht" : "Vorgänge brauchen"} Aufmerksamkeit`,
  financePendingApprovals: "Offene Freigaben",
  financeNeedsDecision: "Ausgaben, die auf eine Entscheidung warten",
  financeToReimburse: "Zu erstatten",
  financeReadyToPay: "Freigegebene Mitarbeiterausgaben zur Zahlung",
  financeUnreconciled: "Nicht abgeglichen",
  financeBankItems: "Bankbuchungen, die noch geprüft werden müssen",
  financeReceivables: "Forderungen",
  financeOpenDocuments: (count: number) => `${count} offene ${count === 1 ? "Position" : "Positionen"}`,
  financeNeedsAttention: "Aufmerksamkeit erforderlich",
  financeNeedsAttentionHint: "Eine zentrale Liste für alles, was saubere Bücher und gesunde Liquidität gefährdet.",
  financeBanking: "Bank",
  financeLatestStatement: "Neuester Kontoauszug",
  financeNoStatements: "Noch kein Kontoauszug importiert.",
  financeStatementLines: (count: number) => `${count} ${count === 1 ? "Transaktion" : "Transaktionen"}`,
  financeClosingBalance: "Schlusssaldo des Kontoauszugs",
  financeOpenBanking: "Bank öffnen",
  moduleInventory: "Lager",
  moduleHr: "Personen",
  moduleCampaigns: "Kampagnen",
  billingWorkspacePurpose:
    "Kunden, Angebote, Rechnungen und Zahlungen in einem gemeinsamen Finanz-Arbeitsbereich.",

  // Home dashboard
  homeGreetingMorning: "Guten Morgen",
  homeGreetingAfternoon: "Guten Tag",
  homeGreetingEvening: "Guten Abend",
  homeWelcome: "Willkommen bei alo workplace",
  homeStatUnreadEmails: "Ungelesene E-Mails",
  homeStatEvents: "Anstehende Termine",
  homeStatMessages: "Ungelesene Nachrichten",
  homeStatFiles: "Dokumente",
  homeStatTasks: "Heute fällige Aufgaben",
  homeSubtitle: "Das steht heute an.",
  homeToolsTitle: "Ihre Tools",
  homeToolsSubtitle:
    "Die Apps, die Sie am häufigsten nutzen — bereit, wenn Sie sie brauchen.",
  homeSearchPlaceholder: "E-Mails, Termine, Aufgaben durchsuchen…",
  homeNotifications: "Benachrichtigungen",
  homeTodaysCalendar: "Kalender für heute",
  homeViewFullCalendar: "Ganzen Kalender öffnen",
  homeNoEventsToday: "Heute steht nichts in Ihrem Kalender.",
  homeMyTasks: "Meine Aufgaben",
  homeViewAllTasks: "Alle Aufgaben anzeigen",
  homeNoTasks: "Nichts fällig. Alles erledigt.",
  homeTaskOverdue: "Überfällig",
  homeTaskToday: "Heute",
  agendaUntitledEvent: "Termin ohne Titel",
  homeGoToMail: "Zu E-Mail",
  homeViewTasks: "Aufgaben anzeigen",
  homeViewCalendar: "Kalender anzeigen",
  homeGoToTasks: "Aufgaben öffnen",
  homeNewTask: "Neue Aufgabe",
  homeViewAgenda: "Zum Kalender",
  homeOpenChat: "Chat öffnen",
  homeOpenDrive: "Drive öffnen",
  homeComingSoonShort: "Bald verfügbar",
  homeRecent: "Zuletzt",
  homeStarred: "Gekennzeichnet",
  homeUnread: "Ungelesen",
  homeViewAll: "Alle anzeigen",
  homeNoRecent: "Hier ist noch nichts.",
  homeQuickActions: "Schnellaktionen",
  homeCompose: "Schreiben",
  homeCreateEvent: "Termin erstellen",
  homeStartChat: "Chat starten",
  homeUploadFile: "Datei hochladen",
  homeCreateDoc: "Dokument erstellen",
  homeToday: "Heute",
  homeAgendaComingSoon: "Ihr Kalender erscheint hier, sobald er verfügbar ist.",
  homeAskTitle: "Fragen Sie alo",
  homeAskBody: "Ihr KI-Assistent für alles im Arbeitsalltag.",
  homeAskCta: "alo fragen",
  homeAskPlaceholder: "Fragen Sie mich etwas…",
  homeAskUnavailable:
    "alo ist gerade nicht erreichbar. Bitte versuchen Sie es gleich noch einmal.",
  homeMailClearTitle: "Alles erledigt!",
  homeCalendarClearTitle: "Heute keine Termine",
  homeTasksClearTitle: "Alles geschafft!",

  // shell
  newButton: "Neu",
  appLauncher: "Apps",
  appLauncherAutoHint: "Ihre meistgenutzten Apps, automatisch aktuell gehalten",
  appLauncherFavorites: "Ihre Favoriten",
  appLauncherAll: "Alle Apps",
  appLauncherMore: "Weitere Apps",
  appLauncherEdit: "Favoriten bearbeiten",
  appLauncherDone: "Fertig",
  appLauncherCancel: "Abbrechen",
  appLauncherDragHint:
    "Ordnen Sie Ihre sechs Lieblings-Apps per Ziehen und Ablegen",
  appLauncherAddFavorite: "Zu Favoriten hinzufügen",
  appLauncherRemoveFavorite: "Aus Favoriten entfernen",
  userMenu: "Konto",
  language: "Sprache",
  signOut: "Abmelden",

  // contacts (address book)
  contactsTitle: "Kontakte",
  contactsOpen: "Kontakte",
  contactsSearchPlaceholder: "Kontakte durchsuchen…",
  contactsEmpty: "Noch keine Kontakte. Legen Sie den ersten an.",
  contactsSearchEmpty: "Keine Kontakte entsprechen Ihrer Suche.",
  contactsLoadError: "Ihre Kontakte konnten nicht geladen werden.",
  contactsNew: "Neuer Kontakt",
  contactEdit: "Kontakt bearbeiten",
  contactFirstName: "Vorname",
  contactLastName: "Nachname",
  contactDisplayName: "Anzeigename",
  contactEmail: "E-Mail",
  contactPhone: "Telefon",
  contactOrganization: "Organisation",
  contactJobTitle: "Position",
  contactNotes: "Notizen",
  contactAddEmail: "E-Mail-Adresse hinzufügen",
  contactAddPhone: "Telefonnummer hinzufügen",
  contactRemoveFieldNamed: (value: string) => `${value} entfernen`,
  contactKindLabel: (value: string) => `Art von ${value}`,
  contactKindWork: "Geschäftlich",
  contactKindHome: "Privat",
  contactKindMobile: "Mobil",
  contactKindOther: "Sonstige",
  contactSave: "Speichern",
  contactCancel: "Abbrechen",
  contactDelete: "Löschen",
  contactDeleteConfirm: (name: string) =>
    `${name} löschen? Das lässt sich nicht rückgängig machen.`,
  contactNeedsName:
    "Geben Sie einen Namen oder mindestens eine E-Mail-Adresse an.",
  contactSaveError: "Dieser Kontakt konnte nicht gespeichert werden.",
  contactDeleteError: "Dieser Kontakt konnte nicht gelöscht werden.",
  contactNoEmail: "Keine E-Mail-Adresse",
  contactsImport: "Importieren",
  contactsExport: "Exportieren",
  contactsImporting: "Import läuft…",
  contactsImported: (n: number, skipped: number) =>
    skipped > 0
      ? `${n} Kontakt${n === 1 ? "" : "e"} importiert (${skipped} übersprungen).`
      : `${n} Kontakt${n === 1 ? "" : "e"} importiert.`,
  contactsImportError:
    "Diese Datei konnte nicht importiert werden. Ist es ein .vcf-Export?",
  contactsExportError: "Ihre Kontakte konnten nicht exportiert werden.",
  contactsExportEmpty: "Sie haben noch keine Kontakte zum Exportieren.",

  // import mail (IMAP wizard)
  importOpen: "E-Mails importieren",
  importTitle: "E-Mails aus einem anderen Konto importieren",
  importIntro:
    "Holen Sie Ihre letzten E-Mails aus Gmail, Outlook oder einem beliebigen IMAP-Konto in Ihren Posteingang.",
  importProvider: "Wo liegen Ihre E-Mails?",
  importProviderGmail: "Gmail",
  importProviderOutlook: "Outlook",
  importProviderOther: "Anderes Konto (IMAP)",
  importServer: "Mailserver",
  importPort: "Port",
  importEmail: "E-Mail-Adresse",
  importPassword: "Passwort",
  importAppPasswordHint:
    "Für Gmail und Outlook brauchen Sie ein App-Passwort, nicht Ihr normales Passwort.",
  importStart: "Import starten",
  importRunning:
    "Ihre E-Mails werden importiert — das kann eine Minute dauern…",
  importDone: (imported: number, skipped: number) =>
    skipped > 0
      ? `${imported} Nachricht${imported === 1 ? "" : "en"} importiert (${skipped} bereits vorhanden).`
      : `${imported} Nachricht${imported === 1 ? "" : "en"} importiert.`,
  importNeedsFields: "Geben Sie Server, E-Mail-Adresse und Passwort an.",
  importClose: "Schließen",
  signedInAs: "Angemeldet als",
  comingSoonTitle: "Bald verfügbar",
  comingSoonBody:
    "Dieser Teil Ihres Arbeitsplatzes ist unterwegs. E-Mail ist schon bereit.",

  // auth — brand panel
  brandHeadline: "Ihr Arbeitsplatz.\nIhre Server.\nIhre Regeln.",
  brandSubtitle:
    "E-Mail, Kalender, Chat und Dateien — souverän, KI-nativ und gehostet in Europa.",
  brandEuBadge: "Auf Ihrer Infrastruktur gehostet · EU",
  // auth — brand panel, standalone mail product (alomails)
  brandHeadlineMail: "Ihre E-Mail.\nIhre Privatsphäre.\nIhre Regeln.",
  brandSubtitleMail:
    "Private, KI-native E-Mail — souverän und gehostet in Europa.",
  brandEuBadgeMail: "Souveräne E-Mail · Gehostet in Europa",
  // auth — brand panel, standalone Drive product (alodrives)
  brandHeadlineDrive: "Ihre Dateien.\nIhre Ordner.\nIhre Regeln.",
  brandSubtitleDrive:
    "Dateien, Ordner und Dokumente an einem Ort — geteilt über ihren Ablageort, und immer Ihre eigenen.",
  brandEuBadgeDrive: "Ihre Dateien, ohne Anbieterbindung",

  // auth — sign in
  signInHeading: "Anmelden",
  signInSubtitle:
    "Willkommen zurück. Geben Sie Ihre Zugangsdaten ein, um fortzufahren.",
  emailLabel: "E-Mail",
  emailPlaceholder: "sie@ihredomain.de",
  emailPlaceholderMail: "sie@alomails.com",
  passwordLabel: "Passwort",
  showPassword: "Passwort anzeigen",
  hidePassword: "Passwort verbergen",
  rememberMe: "Angemeldet bleiben",
  forgotPassword: "Passwort vergessen?",
  forgotPasswordNote:
    "Wenden Sie sich an Ihren Administrator, um Ihr Passwort zurückzusetzen.",
  signInButton: "Anmelden",
  signingIn: "Anmeldung läuft…",
  orDivider: "oder",
  signInWithSso: "Mit SSO anmelden",
  ssoComingSoon: "Single Sign-on kommt bald.",

  // auth — two-factor
  twoFactorTitle: "Zwei-Faktor-Authentifizierung",
  twoFactorSubtitle:
    "Geben Sie den 6-stelligen Code aus Ihrer Authenticator-App ein",
  twoFactorRecoverySubtitle:
    "Geben Sie einen Ihrer Wiederherstellungscodes ein",
  twoFactorCodeLabel: "Authentifizierungscode",
  recoveryCodeLabel: "Wiederherstellungscode",
  recoveryPlaceholder: "xxxx-xxxx",
  verify: "Bestätigen",
  verifying: "Wird geprüft…",
  useRecoveryCode: "Stattdessen einen Wiederherstellungscode verwenden",
  useAuthenticator: "Stattdessen die Authenticator-App verwenden",
  backToSignIn: "Zurück zur Anmeldung",

  // auth — errors
  errorBadCredentials:
    "Diese E-Mail-Adresse oder dieses Passwort stimmt nicht. Bitte versuchen Sie es erneut.",
  errorSecondFactor:
    "Geben Sie Ihren Authentifizierungscode ein, um fortzufahren.",
  errorBadOtp: "Dieser Code stimmt nicht. Bitte versuchen Sie es erneut.",
  errorRateLimited:
    "Zu viele Versuche. Bitte warten Sie einen Moment und versuchen Sie es erneut.",
  errorGeneric:
    "Bei der Anmeldung ist etwas schiefgelaufen. Bitte versuchen Sie es erneut.",
  errorNetwork:
    "Der Server ist nicht erreichbar. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  signingOut: "Abmeldung läuft…",

  // signup — personal accounts (ADR 0018)
  signupHeading: "Erstellen Sie Ihre persönliche alo-Adresse",
  signupSubtitle:
    "Private, souveräne E-Mail — ohne Werbung, ohne Tracking. Für immer.",
  signupAddressLabel: "Wählen Sie Ihre Adresse",
  signupPickPlaceholder: "ihrname",
  signupRecoveryLabel: "Ihre aktuelle E-Mail-Adresse",
  signupRecoveryHint:
    "Wir senden Ihnen dorthin einen Bestätigungscode — sie wird zugleich Ihre Adresse für die Kontowiederherstellung.",
  signupSendCode: "Bestätigungscode senden",
  signupSending: "Wird gesendet…",
  signupChecking: "Wird geprüft…",
  signupAvailable: "Diese Adresse ist frei",
  signupTaken: "Diese Adresse ist bereits vergeben",
  signupReserved: "Diese Adresse ist reserviert",
  signupInvalid:
    "Verwenden Sie 3–64 Buchstaben, Ziffern, Punkte oder Bindestriche",
  signupVerifyHeading: "Code eingeben",
  signupVerifySubtitle: (recovery: string) =>
    `Wir haben einen 6-stelligen Code an ${recovery} gesendet. Er läuft in 10 Minuten ab.`,
  signupCodeLabel: "Bestätigungscode",
  signupPasswordLabel: "Wählen Sie ein Passwort",
  signupPasswordHint: "Mindestens 8 Zeichen.",
  signupCreate: "Konto erstellen",
  signupCreating: "Ihr Konto wird erstellt…",
  signupResend: "Code erneut senden",
  signupVerifyError:
    "Dieser Code ist falsch oder abgelaufen. Bitte versuchen Sie es erneut.",
  signupBeginError:
    "Der Code konnte nicht gesendet werden. Bitte versuchen Sie es erneut.",
  signupDoneHeading: "Alles bereit",
  signupDoneBody: (email: string) =>
    `${email} ist eingerichtet. Melden Sie sich mit Ihrer neuen Adresse und Ihrem Passwort an.`,
  signupGoToLogin: "Zur Anmeldung",
  signupUnavailable: "Persönliche Registrierungen sind derzeit nicht möglich.",
  signupHaveAccount: "Sie haben schon ein Konto?",
  signupBackToLogin: "Anmelden",
  signupCreateLink: "Persönliches Konto erstellen",

  // auth — password reset
  resetHeading: "Passwort zurücksetzen",
  resetSubtitle:
    "Geben Sie Ihre alo-Adresse ein — wir senden einen Code an Ihr Wiederherstellungspostfach.",
  resetAddressLabel: "Ihre alo-Adresse",
  resetSendCode: "Code zum Zurücksetzen senden",
  resetSending: "Wird gesendet…",
  resetVerifyHeading: "Code eingeben",
  resetVerifySubtitle: (address: string) =>
    `Falls ${address} ein alo-Konto hat, ist ein Code an das Wiederherstellungspostfach unterwegs. Geben Sie ihn unten zusammen mit einem neuen Passwort ein.`,
  resetNewPasswordLabel: "Neues Passwort",
  resetSubmit: "Neues Passwort speichern",
  resetSubmitting: "Wird gespeichert…",
  resetDoneHeading: "Passwort aktualisiert",
  resetDoneBody: "Sie können sich jetzt mit Ihrem neuen Passwort anmelden.",
  resetRequestError:
    "Das Zurücksetzen konnte nicht gestartet werden. Bitte versuchen Sie es erneut.",
  resetVerifyError:
    "Das hat nicht geklappt — prüfen Sie den Code und versuchen Sie es erneut.",

  // agenda (calendar)
  agendaNewEvent: "Neuer Termin",
  agendaCalendars: "Kalender",
  agendaMyCalendars: "Meine Kalender",
  agendaOtherCalendars: "Weitere Kalender",
  agendaDay: "Tag",
  agendaAgenda: "Agenda",
  agendaTomorrow: "Morgen",
  agendaUpcoming: "Demnächst",
  agendaNothingUpcoming: "Nichts geplant.",
  agendaEventCount: (n: number) => (n === 1 ? "1 Termin" : `${n} Termine`),
  agendaCalendar: "Kalender",
  agendaNewCalendar: "Neuer Kalender",
  agendaNewCalendarPrompt: "Name des neuen Kalenders",
  agendaDeleteCalendar: "Kalender löschen",
  agendaToday: "Heute",
  agendaPrev: "Zurück",
  agendaToolbarLabel: "Kalender",
  agendaViewLabel: "Ansicht",
  agendaNext: "Weiter",
  agendaMonth: "Monat",
  agendaWeek: "Woche",
  agendaAllDay: "Ganztägig",
  agendaAway: "Abwesend",
  agendaAwayTitle: (names: string) => `Abwesend: ${names}`,
  agendaEventTitle: "Titel hinzufügen",
  agendaEventStart: "Beginn",
  agendaEventEnd: "Ende",
  agendaEventLocation: "Ort",
  rsvpFrom: "Von",
  rsvpAccept: "Zusagen",
  rsvpMaybe: "Vielleicht",
  rsvpDecline: "Absagen",
  rsvpAccepted: "Sie haben diese Einladung angenommen.",
  rsvpDeclined: "Sie haben diese Einladung abgelehnt.",
  rsvpTentative: "Sie haben mit Vielleicht geantwortet.",
  replyResponded: "hat geantwortet",
  replyFrom: (who: string, verb: string) => `${who} ${verb}`,
  replyApplied: "In Ihrem Termin aktualisiert.",
  rsvpError:
    "Ihre Antwort konnte nicht gesendet werden — bitte versuchen Sie es erneut.",
  cancelledTitle: "Abgesagt:",
  cancelledRemoved: "Aus Ihrem Kalender entfernt.",
  cancelledAbsent: "Dieser Termin stand nicht in Ihrem Kalender.",
  agendaEventGuests: "Gäste",
  agendaGuestsPlaceholder: "name@beispiel.de, weitere@beispiel.de",
  agendaGuestsHint:
    "Wir senden jedem Gast eine Einladung per E-Mail, die sich im eigenen Kalender annehmen lässt.",
  agendaEventDescription: "Notizen",
  agendaSave: "Speichern",
  agendaSaveThis: "Diesen Termin",
  agendaSaveAll: "Alle Termine",
  agendaDelete: "Löschen",
  agendaDeleteThis: "Diesen Termin",
  agendaDeleteAll: "Alle Termine",
  agendaCancel: "Abbrechen",
  agendaNewEventTitle: "Neuer Termin",
  agendaNewEventSubtitle: "Erstellen Sie einen neuen Termin in Ihrem Kalender",
  agendaEditEventSubtitle: "Passen Sie die Details Ihres Termins an",
  agendaCreateEvent: "Termin erstellen",
  agendaLocationPlaceholder: "Ort oder Link zur Videokonferenz hinzufügen",
  agendaDescriptionPlaceholder: "Notizen, Agenda oder Details hinzufügen…",
  agendaEditEventTitle: "Termin bearbeiten",
  agendaEndBeforeStart: "Der Termin endet, bevor er beginnt.",
  agendaSaveError:
    "Der Termin konnte nicht gespeichert werden. Bitte versuchen Sie es erneut.",
  agendaRepeat: "Wiederholen",
  agendaRepeatNone: "Keine Wiederholung",
  agendaRepeatDaily: "Täglich",
  agendaRepeatWeekly: "Wöchentlich",
  agendaRepeatWeekdays: "Werktags (Mo–Fr)",
  agendaRepeatMonthly: "Monatlich",
  agendaRepeatYearly: "Jährlich",

  // tasks
  taskProjects: "Projekte",
  taskNewProject: "Neues Projekt",
  taskNewProjectPrompt: "Name des neuen Projekts",
  taskMyPlate: "Auf meinem Tisch",
  taskProposals: "Vorschläge",
  taskBoard: "Board",
  taskList: "Liste",
  taskQuickAdd: "Aufgabe hinzufügen…",
  taskAdd: "Hinzufügen",
  taskColReview: "Prüfung",
  taskOverview: "Überblick",
  taskOvTotal: "Gesamt",
  taskOvCompleted: "Erledigt",
  taskOvProgress: "Fortschritt",
  taskOvByAssignee: "Aufgaben nach Zuständigkeit",
  taskOvUpcoming: "Anstehende Aufgaben",
  taskOvViewAll: "Alle anzeigen",
  taskOvTasksTotal: (n: number) => `${n} Aufgaben insgesamt`,
  taskOvCompletedLabel: "Erledigt",
  taskSummaryTotal: (count: number) => `${count} gesamt`,
  taskSummaryActive: (count: number) => `${count} offen`,
  taskSummaryOverdue: (count: number) => `${count} überfällig`,
  taskSummaryCompleted: (count: number) => `${count} erledigt`,
  taskOvNobody: "Nicht zugewiesen",
  taskColName: "Aufgabe",
  taskColProject: "Projekt",
  taskColAssignee: "Zuständig",
  taskColDue: "Fällig am",
  taskColPriority: "Priorität",
  taskAssigneeYou: "Sie",
  taskMarkDone: "Als erledigt markieren",
  taskMarkNotDone: "Als nicht erledigt markieren",
  taskNew: "Neue Aufgabe",
  taskSearchPlaceholder: "Aufgaben, Projekte durchsuchen…",
  taskEmptyTitle: "Noch keine Aufgaben 👋",
  taskEmptyBody: "Alles bereit. Legen Sie Ihre erste Aufgabe an.",
  taskCreateFirst: "Erste Aufgabe erstellen",
  taskShowProjects: "Projekte einblenden",
  taskHideProjects: "Projekte ausblenden",
  taskNewTaskPrompt: "Name der neuen Aufgabe",
  taskNewSubtitle: "Erstellen Sie eine Aufgabe und behalten Sie den Überblick.",
  taskNamePlaceholder: "z. B. Landingpage gestalten",
  taskCancel: "Abbrechen",
  taskCreate: "Aufgabe erstellen",
  taskCreating: "Wird erstellt…",
  taskAttachments: "Anhänge",
  taskAddAttachment: "Anhang hinzufügen",
  taskFilesEmpty:
    "Noch keine Dateien. Hängen Sie eine an eine beliebige Aufgabe an.",
  taskFilesAttachTo: "An Aufgabe anhängen",
  taskFilesDropHint:
    "Ziehen Sie Bilder oder Dateien hierher, oder nutzen Sie „Anhang hinzufügen“.",
  taskFilesNeedTask:
    "Erstellen Sie zuerst eine Aufgabe — dann können Sie Bilder und Dateien anhängen.",
  taskFilesUploadError:
    "Diese Dateien konnten nicht angehängt werden. Bitte versuchen Sie es erneut.",
  taskChooseFromDrive: "Aus Drive wählen",
  taskChooseFromDriveHint:
    "Vorhandene Dateien anhängen, ohne sie erneut hochzuladen.",
  taskSearchDrive: "In diesem Ordner suchen",
  taskDriveBack: "Zurück zum vorherigen Ordner",
  taskNoDriveFiles: "Keine Dateien in diesem Ordner.",
  taskAttachSelected: "Auswahl anhängen",
  taskFilesSelected: (count: number) =>
    count === 1 ? "1 Datei ausgewählt" : `${count} Dateien ausgewählt`,
  taskCreateOnDate: (date: string) =>
    `Aufgabe mit Fälligkeit ${date} erstellen`,
  taskLabelsTitle: "Labels",
  taskAddLabel: "Label hinzufügen",
  taskNewLabelPlaceholder: "Neues Label…",
  taskCreateLabel: "Erstellen",
  taskFollowers: "Beobachter",
  taskFollow: "Folgen",
  taskLeave: "Aufgabe verlassen",
  taskBlockedBy: "Blockiert durch",
  taskAddBlocker: "Blocker hinzufügen",
  taskNoBlockerCandidates: "Keine anderen Aufgaben als Abhängigkeit verfügbar",
  taskUploading: "Wird hochgeladen…",
  taskDownload: "Herunterladen",
  taskCreateAnother: "Weitere Aufgabe erstellen",
  datePickerClear: "Entfernen",
  datePickerToday: "Heute",
  taskAllTasks: "Alle Aufgaben",
  taskUnassigned: "Nicht zugewiesen",
  taskFilter: "Filtern",
  taskSort: "Sortieren",
  taskGroup: "Gruppieren",
  taskOptions: "Optionen",
  taskSortManual: "Manuell",
  taskSortDue: "Fälligkeitsdatum",
  taskSortPriority: "Priorität",
  taskSortName: "Name",
  taskSortCreated: "Neueste",
  taskGroupStatus: "Status",
  taskGroupProject: "Projekt",
  taskGroupAssignee: "Zuständig",
  taskGroupPriority: "Priorität",
  taskGroupNone: "Keine",
  taskOnlyMine: "Nur meine Aufgaben",
  taskShowCompleted: "Erledigte anzeigen",
  taskCompactRows: "Kompakte Zeilen",
  taskTimeline: "Zeitleiste",
  taskCalendar: "Kalender",
  taskFiles: "Dateien",
  taskUnscheduled: "Ohne Fälligkeitsdatum",
  taskColTodo: "Zu erledigen",
  taskColInProgress: "In Arbeit",
  taskColDone: "Erledigt",
  taskDueToday: "Heute",
  taskDueTomorrow: "Morgen",
  taskDueYesterday: "Gestern",
  taskPrioNone: "Keine",
  taskPrioLow: "Niedrig",
  taskPrioMedium: "Mittel",
  taskPrioHigh: "Hoch",
  taskFromEmail: "Aus einer E-Mail",
  taskFromEvent: "Aus einem Termin",
  taskOpenEmail: "Ursprüngliche E-Mail öffnen",
  createTask: "Aufgabe erstellen",
  suggestTasks: "Aufgaben aus dieser E-Mail vorschlagen",
  taskCreatedFromMail: "Aufgabe aus dieser E-Mail erstellt.",
  taskSuggesting: "Die E-Mail wird nach Aufgaben durchsucht…",
  taskNoSuggestions: "Keine Aufgaben in dieser E-Mail gefunden.",
  taskSuggested: (n: number) =>
    n === 1
      ? "1 Vorschlag zu Ihrem Aufgabeneingang hinzugefügt."
      : `${n} Vorschläge zu Ihrem Aufgabeneingang hinzugefügt.`,
  taskAiOff: "KI ist ausgeschaltet, daher konnte nichts vorgeschlagen werden.",
  taskClose: "Schließen",
  taskDelete: "Löschen",
  taskDetailDialog: "Aufgabendetails",
  taskStatus: "Status",
  taskTimeTracking: "Zeiterfassung",
  taskTimeTrackingHint:
    "Erfassen Sie diese Aufgabe direkt in Ihrem Stundenzettel.",
  taskTimerRunningOnTask: "Für diese Aufgabe läuft die Zeiterfassung.",
  taskTimerRunningElsewhere: "Es läuft bereits ein anderer Timer.",
  taskSwitchTimer: "Timer wechseln",
  taskAssignee: "Zuständig",
  taskAssigneePlaceholder: "name@beispiel.de",
  taskDue: "Fällig",
  taskPriority: "Priorität",
  taskDescription: "Beschreibung",
  taskDescriptionPlaceholder: "Mehr Details hinzufügen…",
  taskSubtasks: "Teilaufgaben",
  taskAddSubtask: "Teilaufgabe hinzufügen…",
  taskComments: "Kommentare",
  taskAddComment: "Kommentar schreiben…",
  taskActivity: "Aktivität",
  taskEmpty: "Noch keine Aufgaben. Fügen Sie oben eine hinzu.",
  taskPlateEmpty: "Nichts fällig. Alles erledigt.",
  taskNoProposalsTitle: "Alles erledigt",
  taskNoProposals:
    "Vorschläge erscheinen hier, wenn alo in einer E-Mail Aufgaben findet.",
  taskAiSuggested: "KI-Vorschlag",
  taskAccept: "Übernehmen",
  taskReject: "Verwerfen",
  taskActivityKind: (kind: string) =>
    (
      ({
        created: "hat diese Aufgabe erstellt",
        status_changed: "hat sie verschoben",
        assigned: "hat die Zuständigkeit geändert",
        due_changed: "hat das Fälligkeitsdatum geändert",
        commented: "hat kommentiert",
        accepted: "hat den Vorschlag übernommen",
        proposed: "wurde von der KI vorgeschlagen",
      }) as Record<string, string>
    )[kind] ?? kind,
  agendaReminder: "Erinnerung",
  agendaReminderNone: "Keine Erinnerung",
  agendaReminderAtStart: "Zum Terminbeginn",
  agendaReminder5: "5 Minuten vorher",
  agendaReminder10: "10 Minuten vorher",
  agendaReminder15: "15 Minuten vorher",
  agendaReminder30: "30 Minuten vorher",
  agendaReminder60: "1 Stunde vorher",
  agendaReminder1Day: "1 Tag vorher",
  agendaRsvpAccepted: "Zugesagt",
  agendaRsvpDeclined: "Abgesagt",
  agendaRsvpTentative: "Vielleicht",
  agendaRsvpPending: "Noch keine Antwort",
  agendaCheckAvailability: "Verfügbarkeit prüfen",
  agendaAvailChecking: "Wird geprüft…",
  agendaAvailAllFree: "Alle sind dann frei.",
  agendaAvailBusy: (names: string) => `Dann beschäftigt: ${names}`,
  agendaAvailOutside: (names: string) =>
    `Dann außerhalb der Arbeitszeiten: ${names}`,
  agendaRoom: "Raum",
  agendaRoomNone: "Kein Raum",
  agendaRoomHint:
    "Der Raum wird mit dem Termin eingeladen und für dessen Zeit reserviert.",
  agendaRoomSeats: (seats: number) => `${seats} Plätze`,
  agendaRoomTaken: (name: string) => `${name} ist dann schon belegt.`,
  agendaWorkingHours: "Arbeitszeiten",
  agendaWorkingHoursHint:
    "Wer einen Termin mit Ihnen plant, sieht Zeiten außerhalb dieser Stunden markiert.",
  agendaWorkingDays: "Arbeitstage",
  agendaWorkStart: "Beginn",
  agendaWorkEnd: "Ende",
  agendaWorkZone: "Zeitzone",
  agendaWorkZoneMine: "Meine Zeitzone",
  agendaWorkHoursOrder: "Die Arbeitszeiten enden, bevor sie beginnen.",
  agendaWorkingHoursError:
    "Ihre Arbeitszeiten konnten nicht gespeichert werden. Bitte versuchen Sie es erneut.",
  agendaWorkingHoursLoadError:
    "Ihre Arbeitszeiten konnten nicht geladen werden.",
  agendaAvailNoGuests:
    "Fügen Sie Gäste hinzu, um ihre Verfügbarkeit zu prüfen.",
  agendaAvailError: "Verfügbarkeit konnte nicht geprüft werden.",
  agendaClose: "Schließen",
  agendaReadOnly: "Sie können diesen Kalender nur ansehen.",
  // Calendar sharing
  agendaShare: "Kalender freigeben",
  agendaShareTitle: (name: string) => `„${name}“ freigeben`,
  agendaShareWith: "Freigeben für",
  agendaSharePerson: "Eine Person",
  agendaShareGroupOption: "Eine Gruppe",
  agendaShareEmail: "E-Mail-Adresse",
  agendaShareEmailPlaceholder: "name@beispiel.de",
  agendaShareGroupPick: "Gruppe wählen…",
  agendaShareAccess: "Zugriff",
  agendaShareViewer: "Darf ansehen",
  agendaShareEditor: "Darf bearbeiten",
  agendaShareGroup: "Gruppe",
  agendaShareAdd: "Freigeben",
  agendaShareRemove: "Entfernen",
  agendaShareRemoveFor: (name: string) => `Freigabe für ${name} beenden`,
  agendaShareEmpty: "Noch für niemanden freigegeben.",
  agendaShareLoadError:
    "Es konnte nicht geladen werden, für wen dieser Kalender freigegeben ist.",
  agendaShareError:
    "Die Freigabe konnte nicht aktualisiert werden. Bitte versuchen Sie es erneut.",

  // mail
  mailLoading: "Ihre E-Mails werden geladen…",
  mailSearching: "Suche läuft…",
  mailFolders: "Ordner",
  flaggedView: "Gekennzeichnet",
  // Flag follow-up due-date
  flagDueAdd: "Fälligkeitsdatum hinzufügen",
  flagDueToday: "Heute",
  flagDueTomorrow: "Morgen",
  flagDueNextWeek: "Nächste Woche",
  flagDuePick: "Datum wählen…",
  flagDueClear: "Fälligkeitsdatum entfernen",
  flagDueLabel: (when: string) => `Fällig ${when}`,
  flagDueOverdue: (when: string) => `Überfällig — war fällig ${when}`,
  flagDueSet: "Datum zur Nachverfolgung festlegen",
  resizeFolders:
    "Ordnerbereich anpassen (ziehen oder Pfeiltasten; Doppelklick zum Zurücksetzen)",
  resizeMessages:
    "Nachrichtenliste anpassen (ziehen oder Pfeiltasten; Doppelklick zum Zurücksetzen)",
  collapseFolders: "Ordner ausblenden",
  expandFolders: "Ordner einblenden",
  mailEmpty: "Hier sind noch keine Nachrichten.",
  mailSearchEmpty: "Keine Nachrichten entsprechen Ihrer Suche.",
  mailSelectPrompt: "Ihr Posteingang ist bereit",
  mailSelectBody:
    "Wählen Sie eine Nachricht aus der Liste, um die Unterhaltung zu öffnen.",
  mailListError: "Nachrichten konnten nicht geladen werden.",
  mailFolderError: "Ihre Ordner konnten nicht geladen werden.",
  mailRetry: "Erneut versuchen",
  mailFrom: "Von",
  mailTo: "An",
  mailNoSubject: "(kein Betreff)",
  mailUnknownSender: "Unbekannter Absender",

  // mail — sidebar
  compose: "Schreiben",
  mailSearchPlaceholder: "E-Mails durchsuchen…",
  viewAsMessages: "Als einzelne Nachrichten anzeigen",
  viewAsConversations: "Als Unterhaltungen anzeigen",

  // mail — reading pane
  conversationActions: "Aktionen für die Unterhaltung",
  reply: "Antworten",
  replyAll: "Allen antworten",
  forward: "Weiterleiten",
  archive: "Archivieren",
  snooze: "Zurückstellen",
  flag: "Kennzeichnen",
  unflag: "Kennzeichnung entfernen",
  markRead: "Als gelesen markieren",
  markUnread: "Als ungelesen markieren",
  selectAll: "Alle auswählen",
  selectNone: "Auswahl aufheben",
  selectedCount: (n: number) => (n === 1 ? "1 ausgewählt" : `${n} ausgewählt`),
  snoozeUntil: "Zurückstellen bis…",
  snoozeLaterToday: "Später heute",
  snoozeTomorrow: "Morgen",
  snoozeWeekend: "Dieses Wochenende",
  snoozeNextWeek: "Nächste Woche",
  mailSnoozed: "Zurückgestellt",
  delete: "Löschen",
  dialogConfirm: "Bestätigen",
  dialogCancel: "Abbrechen",
  dialogOk: "OK",
  deletePermanently: "Endgültig löschen",
  moveTo: "In Ordner verschieben",
  moreActions: "Weitere Aktionen",
  mailMoved: "Nachricht verschoben.",
  mailDeleted: "Nachricht gelöscht.",
  mailActionFailed: "Das hat nicht geklappt — bitte versuchen Sie es erneut.",
  endOfMessage: "Ende der Nachricht",
  threadMessages: "Nachrichten",
  aloSummary: "alo-Zusammenfassung",
  summaryPending: "Unterhaltung wird zusammengefasst…",
  smartReplies: "Antwortvorschläge",
  quickReplyHint: "Allen antworten · Weiterleiten oben",
  toLabel: "an",
  ccLabel: "cc",
  bccLabel: "bcc",
  recipientsNone: "—",
  senderVerified: "Verifiziert",
  senderVerifiedTitle:
    "Absender authentifiziert — SPF, DKIM und DMARC bestanden",
  replyTo: "Antworten an",
  quickReplyTo: (name: string) => `Schnellantwort an ${name}`,
  replyToName: (name: string) => `${name} antworten…`,
  draftWithAi: "Mit KI entwerfen",
  attachments: "Anhänge",
  attach: "Dateien anhängen",
  attachmentUploading: "Wird hochgeladen…",
  attachmentDownloading: "Wird heruntergeladen…",
  attachmentUploadFailed: "Diese Datei konnte nicht hochgeladen werden.",
  downloadAttachment: (name: string) => `${name} herunterladen`,
  attachmentFailed: "Dieser Anhang konnte nicht heruntergeladen werden.",

  // mail — compose
  composeTitle: "Neue Nachricht",
  composeEdit: "Entwurf bearbeiten",
  composeEditTitle: "Nachricht bearbeiten",
  composeReplyTitle: "Antworten",
  composeForwardTitle: "Weiterleiten",
  composeForwardPrefix: "Fwd: ",
  composeForwardedIntro: "---------- Weitergeleitete Nachricht ----------",
  composeLabelFrom: "Von:",
  composeLabelDate: "Datum:",
  composeLabelSubject: "Betreff:",
  composeLabelTo: "An:",
  composeReplyAllTitle: "Allen antworten",
  composeFrom: "Von",
  composeTo: "An",
  composeCc: "Cc",
  composeBcc: "Bcc",
  composeSubject: "Betreff",
  composeRecipientsPlaceholder: "name@beispiel.de, …",
  composeSubjectPlaceholder: "Betreff",
  composeBodyPlaceholder: "Nachricht schreiben…",
  composeSend: "Senden",
  composeSending: "Wird gesendet…",
  composeDiscard: "Verwerfen",
  composeCcToggle: "Cc",
  composeNoRecipients: "Fügen Sie mindestens einen Empfänger hinzu.",
  composeSendError:
    "Ihre Nachricht konnte nicht gesendet werden. Bitte versuchen Sie es erneut.",
  composeSent: "Nachricht gesendet.",
  composeUndoWindow: "Wird gesendet…",
  composeUndoSend: "Rückgängig",
  composeSendUndone:
    "Senden rückgängig gemacht — Ihre Nachricht liegt in Entwürfe.",
  scheduleSend: "Später senden",
  scheduleTomorrowMorning: "Morgen früh",
  scheduleTomorrowAfternoon: "Morgen Nachmittag",
  scheduleMondayMorning: "Montagmorgen",
  schedulePickTime: "Datum und Uhrzeit wählen",
  mailScheduled: (when: string) => `Senden geplant für ${when}.`,
  scheduleError:
    "Ihre Nachricht konnte nicht geplant werden. Bitte versuchen Sie es erneut.",
  cancelSend: "Senden abbrechen",
  sendCancelled:
    "Geplantes Senden abgebrochen — Ihre Nachricht liegt wieder in Entwürfe.",
  contactSuggestions: "Passende Kontakte",
  labelColor: "Labelfarbe",
  labelColorHint: "Rechtsklick zum Einfärben",
  labelColorClear: "Keine Farbe",
  folderNew: "Neuer Ordner",
  folderNewSub: "Neuer Unterordner",
  folderRename: "Umbenennen",
  folderDelete: "Ordner löschen",
  folderNamePlaceholder: "Ordnername",
  folderDeleteConfirm: (name: string) =>
    `Ordner „${name}“ löschen? Die Nachrichten darin werden nicht gelöscht.`,
  folderActionFailed:
    "Diese Ordneränderung hat nicht geklappt — bitte versuchen Sie es erneut.",
  folderActions: (name: string) => `Optionen für den Ordner ${name}`,
  // Shared mailboxes / delegation
  sharedMailboxLabel: "Postfach",
  sharedMailboxesHeading: "Freigegebene Postfächer",
  sharedMyMailbox: "Mein Postfach",
  sharedReadOnly: "schreibgeschützt",
  sharedNoSend:
    "Aus diesem freigegebenen Postfach können Sie nicht senden — Ihnen wurde kein Senderecht erteilt.",
  // Self-service sharing (Settings)
  settingsSharing: "Freigabe",
  settingsSharingHint:
    "Lassen Sie Kolleginnen und Kollegen Ihr Postfach öffnen und verwalten. Erteilen Sie Senderecht, damit sie auch als Sie senden können.",
  sharingNone: "Sie haben Ihr Postfach für niemanden freigegeben.",
  sharingEmailPlaceholder: "E-Mail der Kollegin oder des Kollegen",
  sharingAdd: "Freigeben",
  sharingAddError:
    "Freigabe fehlgeschlagen — prüfen Sie, ob die E-Mail-Adresse zu einer Person in Ihrer Organisation gehört.",
  // App-specific passwords (Settings)
  settingsAppPasswords: "App-Passwörter",
  settingsAppPasswordsHint:
    "Passwörter für Mail-Apps, die sich klassisch anmelden (IMAP, POP3, SMTP) — etwa Thunderbird oder die Mail-App auf Ihrem Telefon. Jede App bekommt ihr eigenes Passwort, das Sie einzeln widerrufen können, ohne Ihr Kontopasswort zu ändern.",
  appPasswordNone:
    "Noch keine App-Passwörter. Erstellen Sie eines, wenn eine Mail-App nach einem Passwort fragt — besonders wenn Ihr Konto die Anmeldung in zwei Schritten nutzt, die klassische Mail-Apps nicht beherrschen.",
  appPasswordNamePlaceholder:
    "Wofür ist es? z. B. Thunderbird am Schreibtischrechner",
  appPasswordCreate: "Passwort erstellen",
  appPasswordCreated: (date: string) => `Erstellt am ${date}`,
  appPasswordLastUsed: (date: string) => `Zuletzt verwendet am ${date}`,
  appPasswordNeverUsed: "Noch nie verwendet",
  appPasswordRevokeFor: (name: string) => `${name} widerrufen`,
  appPasswordSecretFor: (name: string) => `Passwort für „${name}“`,
  appPasswordSecretHint:
    "Kopieren Sie es jetzt in die App — zu Ihrer Sicherheit kann es nicht noch einmal angezeigt werden.",
  appPasswordCopy: "Passwort kopieren",
  appPasswordCopied: "Kopiert",
  appPasswordSecretDone: "Fertig",
  appPasswordListError:
    "Ihre App-Passwörter konnten nicht geladen werden — bitte versuchen Sie es erneut.",
  appPasswordCreateError:
    "Das App-Passwort konnte nicht erstellt werden — geben Sie ihm einen kurzen Namen; ein Konto kann höchstens 20 haben.",
  appPasswordRevokeError:
    "Widerruf fehlgeschlagen — bitte versuchen Sie es erneut.",
  // Benachrichtigungen (Web Push, Einstellungen)
  settingsNotifications: "Benachrichtigungen",
  settingsNotificationsHint:
    "Erhalten Sie auf diesem Gerät ein Zeichen, sobald etwas Neues ankommt — auch wenn alo geschlossen ist. Jedes Gerät meldet sich einzeln an, und Sie können jedes hier wieder abschalten.",
  pushLoadError:
    "Ihre Benachrichtigungseinstellungen konnten nicht geladen werden — bitte versuchen Sie es erneut.",
  pushNotAvailable:
    "Benachrichtigungen sind auf diesem Server noch nicht eingeschaltet.",
  pushUnsupported:
    "Dieser Browser kann keine Benachrichtigungen installierter Apps anzeigen.",
  pushThisDevice: "Benachrichtigungen auf diesem Gerät",
  pushOnNote:
    "Ein — Sie erfahren von neuer E-Mail, auch wenn alo nicht geöffnet ist.",
  pushOffNote: "Aus — dieses Gerät bleibt still.",
  pushEnable: "Einschalten",
  pushDisable: "Ausschalten",
  pushPermissionBlocked:
    "Der Browser blockiert Benachrichtigungen für diese Website. Erlauben Sie sie in den Website-Einstellungen des Browsers und versuchen Sie es erneut.",
  pushThisDeviceTag: "Dieses Gerät",
  pushDeviceSince: (date: string) => `Seit ${date}`,
  pushDeviceRemove: (name: string) => `Benachrichtigungen auf ${name} beenden`,
  pushPrivacyNote:
    "Eine Benachrichtigung ist nur ein Zeichen — was ankam, bleibt in alo, bis Sie es öffnen.",
  pushError:
    "Die Benachrichtigungen konnten nicht aktualisiert werden — bitte versuchen Sie es erneut.",
  // Admin — mailbox delegation
  userShareAccess: "Freigegebener Zugriff",
  delegateTitle: (email: string) => `Wer auf ${email} zugreifen kann`,
  delegateIntro:
    "Personen, die Sie hinzufügen, können dieses Postfach öffnen und verwalten. Mit Senderecht können sie auch unter dieser Adresse senden.",
  delegatePeople: "Personen mit Zugriff",
  delegateNone: "Noch hat niemand sonst Zugriff.",
  delegateAdd: "Person hinzufügen",
  delegateReadOnly: "Nur Lesen",
  delegateManage: "Darf verwalten",
  delegateAccessLabel: "Zugriffsstufe",
  delegateSendLabel: "Senderecht",
  delegateSendNone: "Darf nicht senden",
  delegateSendAs: "Senden als",
  delegateSendOnBehalf: "Senden im Auftrag",
  delegateRemove: "Zugriff entfernen",
  delegateRemoveFor: (email: string) => `Zugriff von ${email} entfernen`,
  delegateFoldersFor: (email: string) => `${email} auf Ordner beschränken`,
  delegateError:
    "Diese Zugriffsänderung hat nicht geklappt — bitte versuchen Sie es erneut.",
  // Per-folder access (ADR 0017)
  delegateFoldersLabel: "Auf Ordner beschränken",
  delegateWholeMailbox: "Gesamtes Postfach",
  delegateLimitFolders: "Zugriff auf bestimmte Ordner beschränken",
  delegateFoldersSave: "Ordner speichern",
  delegateFoldersCancel: "Abbrechen",
  // Categories (colored message labels)
  categories: "Kategorien",
  categorize: "Kategorisieren",
  categoryNew: "Neue Kategorie",
  categoryRename: "Umbenennen",
  categoryDelete: "Kategorie löschen",
  categoryNamePlaceholder: "Name der Kategorie",
  categoryNoneHint:
    "Noch keine Kategorien — legen Sie in der Seitenleiste eine an.",
  categoryDeleteConfirm: (name: string) =>
    `Kategorie „${name}“ löschen? Sie wird von allen Nachrichten entfernt, die sie tragen.`,
  categoryActionFailed:
    "Diese Kategorieänderung hat nicht geklappt — bitte versuchen Sie es erneut.",
  categoryActions: (name: string) => `Optionen für die Kategorie ${name}`,
  categoryClearFilter: "Alle Nachrichten anzeigen",
  // alo Transfer (large files as expiring links)
  transferLink: "Link",
  transferSharedFile: "📎 Geteilte Datei",
  transferDownload: "Herunterladen",
  transferExpires: (date: string) => `Link läuft am ${date} ab`,
  transferExpiryTitle: "So lange bleiben Links zu großen Dateien aktiv",
  transferExpiryOption: (days: number) =>
    days === 1 ? "1 Tag" : `${days} Tage`,
  blockSenderNamed: (email: string) => `${email} blockieren`,
  senderBlocked: (email: string) =>
    `${email} blockiert — Mails dieser Adresse landen jetzt im Spam-Ordner.`,
  // Filters & rules
  settingsFilters: "Filter & Regeln",
  settingsFiltersHint:
    "Regeln laufen auf Ihrem Server, sobald E-Mails ankommen — auch wenn Sie offline sind. Die erste passende Regel greift.",
  filtersLoadError: "Ihre Filter konnten nicht geladen werden.",
  filtersSaveError:
    "Ihre Filter konnten nicht gespeichert werden. Bitte versuchen Sie es erneut.",
  filterAddRule: "Regel hinzufügen",
  filterNamePlaceholder: "Name der Regel (optional)",
  filterWhen: "Wenn eine Nachricht ankommt und",
  filterDo: "Dann",
  filterMatchAll: "alle zutreffen",
  filterMatchAny: "eine zutrifft",
  filterOr: "oder",
  filterFieldFrom: "Von",
  filterFieldTo: "An",
  filterFieldCc: "Cc",
  filterFieldSubject: "Betreff",
  filterOpContains: "enthält",
  filterOpIs: "ist genau",
  filterValuePlaceholder: "Wert",
  filterAddCondition: "Bedingung hinzufügen",
  filterRemoveCondition: "Bedingung entfernen",
  filterConditionField: (n: number) => `Bedingung ${n}: Feld`,
  filterConditionOp: (n: number) => `Bedingung ${n}: Vergleich`,
  filterConditionValue: (n: number) => `Bedingung ${n}: Wert`,
  filterRemoveConditionAt: (n: number) => `Bedingung ${n} entfernen`,
  filterRuleEnabled: (rule: string) => `Regel aktiv: ${rule}`,
  filterFolderLabel: "Zielordner",
  filterActionFileInto: "In Ordner verschieben",
  filterActionMarkRead: "Als gelesen markieren",
  filterActionStar: "Kennzeichnen",
  filterActionDelete: "Löschen",
  filterSaveRule: "Regel speichern",
  filterCancel: "Abbrechen",
  filterDelete: "Regel löschen",
  filterNeedsCondition:
    "Fügen Sie mindestens eine Bedingung mit einem Wert hinzu.",
  filterNeedsAction: "Wählen Sie mindestens eine Aktion.",
  composeWroteOn: "schrieb:",
  composeReplyPrefix: "Re: ",
  composeBack: "Zurück",
  composeExpand: "Vollbild",
  composeCollapse: "Vollbild beenden",
  composeMinimize: "Minimieren",
  composeRestore: "Wiederherstellen",
  showQuoted: "Zitierten Text anzeigen",
  showOriginal: "Original anzeigen",
  downloadEml: ".eml herunterladen",
  print: "Drucken",
  reportSpam: "Spam melden",
  notSpam: "Kein Spam",
  // Spam "why was this flagged" banner (reading pane, Junk folder)
  spamBannerTitle: "Diese Nachricht liegt im Spam-Ordner",
  spamReasonDmarc: (domain: string) =>
    `Wir konnten nicht bestätigen, dass sie wirklich von ${domain} stammt — die DMARC-Prüfung ist fehlgeschlagen, ein häufiges Zeichen für Absenderfälschung.`,
  spamReasonDkim:
    "Ihre kryptografische Signatur (DKIM) war ungültig, der Absender ließ sich also nicht verifizieren.",
  spamReasonSpf: (domain: string) =>
    `Der Server, der sie verschickt hat, ist von ${domain} nicht zum E-Mail-Versand zugelassen (SPF fehlgeschlagen).`,
  spamReasonNone:
    "Wir haben kein Zustellproblem an dieser Nachricht erkannt — sie ähnelt womöglich E-Mails, die Sie oder eine Filterregel zuvor als Spam markiert haben.",
  spamBannerHint:
    "Falls das kein Spam ist, verschieben Sie die Nachricht zurück in den Posteingang.",
  spamSenderFallback: "der Absenderdomain",
  // One-click unsubscribe (RFC 8058)
  unsubscribe: "Abbestellen",
  unsubscribeConfirm: (sender: string) =>
    `${sender} abbestellen? Wir bitten den Absender, Ihnen keine E-Mails mehr zu senden.`,
  unsubscribed:
    "Abbestellt — der Absender wurde gebeten, keine weiteren E-Mails zu senden.",
  unsubscribeFailed:
    "Automatisches Abbestellen war nicht möglich — versuchen Sie den Link in der Nachricht.",
  unsubscribeOpened: "Die Abmeldeseite wurde in einem neuen Tab geöffnet.",
  forwardAsAttachment: "Als Anhang weiterleiten",
  blockSender: "Absender blockieren",
  junkUnavailable:
    "Es gibt keinen Spam-Ordner, in den verschoben werden könnte.",
  hideQuoted: "Zitierten Text ausblenden",
  formatting: "Textformatierung",
  bold: "Fett",
  italic: "Kursiv",
  underline: "Unterstrichen",
  link: "Link einfügen",
  linkPrompt: "Link-URL:",
  improve: "Verbessern",
  aiImproveFailed: "Die KI konnte das gerade nicht umformulieren.",

  // mail — send/attach error details (surfaced verbatim with the server's reason)
  mailAttachmentErrorDetail: (reason: string) =>
    `Diese Datei wurde nicht angehängt. Versuchen Sie es noch einmal. Server: ${reason}`,
  mailDraftCreateErrorDetail: (reason: string) =>
    `Ihre Nachricht wurde nicht gesendet, weil ihr Entwurf nicht angelegt werden konnte. Das Verfassen-Fenster ist noch offen; versuchen Sie erneut zu senden. Server: ${reason}`,
  mailSubmitErrorDetail: (reason: string) =>
    `Ihre Nachricht wurde nicht gesendet. Sie liegt weiterhin in Entwürfe — öffnen Sie sie und versuchen Sie es erneut. Server: ${reason}`,
  mailScheduleErrorDetail: (reason: string) =>
    `Ihre Nachricht wurde nicht geplant. Sie liegt weiterhin in Entwürfe — öffnen Sie sie und versuchen Sie es erneut. Server: ${reason}`,
  // compose — insert math/code into an email
  composeInsertEquation: "Formel einfügen",
  composeInsertCode: "Codeblock einfügen",

  // alo Docs surface (document chrome)
  docTitle: "Angebot Q3 — Proceq",
  docSaved: "In Drive gespeichert · alle Änderungen gespeichert",
  docSaving: "Wird gespeichert…",
  docViewMode: "Dokumentansicht",
  docCanvasView: "Canvas",
  docCanvasViewHint: "Flexible Canvas-Ansicht",
  docPageView: "Seite",
  docPageViewHint: "Seitenansicht im Druckformat",
  docFormattingToolbar: "Formatierungsleiste des Dokuments",
  docMenuFile: "Datei",
  docMenuEdit: "Bearbeiten",
  docMenuInsert: "Einfügen",
  docMenuFormat: "Format",
  docPrint: "Drucken",
  docInsertDivider: "Trennlinie",
  docInsertPageBreak: "Seitenumbruch",
  docZoom: "Dokument-Zoom",
  docZoomOut: "Verkleinern",
  docZoomIn: "Vergrößern",
  docParagraphStyle: "Absatzformat",
  docStyleParagraph: "Absatz",
  docStyleHeading1: "Überschrift 1",
  docStyleHeading2: "Überschrift 2",
  docStyleHeading3: "Überschrift 3",
  docStyleBulletList: "Aufzählung",
  docStyleNumberedList: "Nummerierte Liste",
  docStyleChecklist: "Checkliste",
  docTextColor: "Textfarbe",
  docHighlightColor: "Hervorhebungsfarbe",
  docHighlightNone: "Keine Hervorhebung",
  docColorDefault: "Standardfarbe",
  docColorHex: "Hex",
  docColorOpacity: "Deckkraft",
  docColorEyedropper: "Farbe vom Bildschirm aufnehmen",
  docBrandColors: "Markenfarben",
  docSaveBrandColor: "Aktuelle Markenfarbe speichern",
  docRemoveBrandColor: "Markenfarbe entfernen",
  docColorRed: "Rot",
  docColorOrange: "Orange",
  docColorYellow: "Gelb",
  docColorGreen: "Grün",
  docColorBlue: "Blau",
  docColorPurple: "Lila",
  docIndent: "Einzug vergrößern",
  docOutdent: "Einzug verkleinern",
  docWords: "Wörter",
  docCharacters: "Zeichen",
  docInsertLink: "Link einfügen",
  docLinkPrompt: "Webadresse für den markierten Text eingeben",
  docInsertImage: "Bild einfügen",
  docFindReplace: "Suchen und ersetzen",
  docFind: "Suchen",
  docReplaceWith: "Ersetzen durch",
  docFindNext: "Weitersuchen",
  docReplaceAll: "Alle ersetzen",
  docPageSetup: "Seite einrichten",
  docPageSize: "Seitengröße",
  docPageLetter: "Letter",
  docPageOrientation: "Ausrichtung",
  docPagePortrait: "Hochformat",
  docPageLandscape: "Querformat",
  docPageMargins: "Seitenränder",
  docMarginsNormal: "Normal",
  docMarginsNarrow: "Schmal",
  docMarginsWide: "Breit",
  docHeader: "Kopfzeile",
  docHeaderPlaceholder: "Text der Kopfzeile",
  docFooter: "Fußzeile",
  docFooterPlaceholder: "Text der Fußzeile",
  docPageNumbers: "Seitenzahl anzeigen",
  docFontFamily: "Schriftart",
  docFontSize: "Schriftgröße",
  docLineSpacing: "Zeilenabstand",
  docAddComment: "Kommentar hinzufügen",
  docComment: "Kommentar",
  docCommentPlaceholder: "Kommentar schreiben…",
  docResolveComment: "Kommentar als erledigt markieren",
  docReopenComment: "Kommentar wieder öffnen",
  docSavePdf: "Als PDF speichern",
  docAiPlaceholder: "Sagen Sie der KI, was sie schreiben oder ändern soll…",
  docAiPropose: "Entwerfen",
  docAiProposalLabel: "Vorschlag — vor dem Einfügen prüfen",
  docAiInsert: "Einfügen",
  docAiDiscard: "Verwerfen",
  docAiUnavailable: "Die KI ist gerade nicht verfügbar.",
  docAskAi: "KI fragen",
  docEquation: "Formel",
  docEquationHint: "Mathematische Formel (LaTeX)",
  docBlockGroupAdvanced: "Erweitert",
  docShare: "Teilen",
  docInsert: "Einfügen",
  insertEquation: "Formel",
  insertCrossRef: "Querverweis",
  tbNormalText: "Normaler Text",
  tbEditing: "Bearbeitung",

  // the example spec on the page
  specTitle: "Wärmeübertragung im Coateq-Paneel",
  specSubtitle: "Technische Spezifikation · Rev. 3",
  specLead1: "Der stationäre Wärmestrom wird bestimmt durch",
  specLead2: "über die Grenzfläche.",
  specMid:
    "wobei k die Wärmeleitfähigkeit ist und r₁, r₂ den inneren und äußeren Radius bezeichnen. Mit den gemessenen Werten:",
  specBcHeading: "Randbedingungen",
  specRefLead: "Aus",
  specRefMid: "mit den Werten aus",
  specRefTail: "ergeben sich die Zahlen unten.",
  tblSymbol: "Symbol",
  tblValue: "Wert",

  // equation editor (modal)
  eqTitle: "Formel",
  eqClose: "Schließen",
  eqInsert: "Einfügen",
  eqPlaceholder: "z. B.  E = mc^2",
  eqInputLabel: "LaTeX-Quelltext",
  eqPreview: "Vorschau",
  eqEmpty: "Geben Sie oben LaTeX ein.",
  eqError: (message: string) =>
    `Dieses LaTeX kann nicht dargestellt werden: ${message}`,
  eqNumbered: "Nummeriert",
  eqEmptyBlock: "Leere Formel — zum Bearbeiten klicken",
  // equation symbol picker
  eqSearchLabel: "Symbole suchen",
  eqSearchPlaceholder: "Symbole suchen — z. B. sum, alpha, arrow",
  eqSearchClear: "Suche löschen",
  eqNoMatches: "Keine Symbole passen zu Ihrer Suche.",
  eqCatStructures: "Strukturen",
  eqCatStyles: "Schriften & Stile",
  eqCatGreek: "Griechisch",
  eqCatOperators: "Operatoren",
  eqCatRelations: "Relationen",
  eqCatSets: "Mengen & Logik",
  eqCatArrows: "Pfeile",
  eqCatBigops: "Große Operatoren",
  eqCatCalculus: "Analysis",
  eqCatDelimiters: "Klammern",
  eqCatMisc: "Symbole",

  // compose/editor formatting toolbar
  strikethrough: "Durchgestrichen",
  textColor: "Textfarbe",
  highlight: "Hervorheben",
  bulletList: "Aufzählung",
  numberedList: "Nummerierte Liste",
  alignLeft: "Linksbündig",
  alignCenter: "Zentriert",
  alignRight: "Rechtsbündig",
  horizontalRule: "Trennlinie",
  insertImage: "Bild einfügen",
  clearFormatting: "Formatierung entfernen",
  textStyle: "Textstil",
  styleQuote: "Zitat",
  fontFamily: "Schriftart",
  fontSize: "Schriftgröße",
  sizeSmall: "Klein",
  sizeNormal: "Normal",
  sizeLarge: "Groß",
  sizeHuge: "Sehr groß",
  codeInsertTitle: "Codeblock einfügen",
  codeInsertHint: "⌘/Strg + Eingabe zum Einfügen",
  codePreviewLabel: "Vorschau — so erscheint es in der E-Mail",
  insertCancel: "Abbrechen",
  insertConfirm: "Einfügen",

  // Docs — the module (browser + editor chrome)
  docsTitle: "alo Docs",
  docsNew: "Neues Dokument",
  docsEmpty: "Noch keine Dokumente. Erstellen Sie eines, um loszuschreiben.",
  docsDelete: (title: string) => `${title} löschen`,
  docsAll: "Alle Dokumente",
  docsUntitled: "Unbenanntes Dokument",
  docsTitleLabel: "Dokumenttitel",
  docsSaving: "Wird gespeichert…",
  docsSaved: "Gespeichert",
  docsSaveError: "Speichern fehlgeschlagen",

  // Docs — block editor controls
  blockAdd: "Block hinzufügen",
  blockMoveUp: "Block nach oben verschieben",
  blockMoveDown: "Block nach unten verschieben",
  blockDelete: "Block löschen",
  blockEmptyHint:
    "Beginnen Sie mit einer Überschrift, Text, einer Formel, Code oder einer Tabelle.",

  // heading block
  headingH1: "Überschrift 1",
  headingH2: "Überschrift 2",
  headingPlaceholder: "Abschnittsüberschrift",
  headingLabel: "Überschriftstext",

  // paragraph block
  paraPlaceholder:
    "Schreiben Sie hier. Über die Werkzeugleiste fügen Sie Formeln im Text oder Querverweise ein.",
  paraLabel: "Absatztext",
  paraInlineMath: "Formel im Text",
  paraReference: "Verweis",
  paraToolbar: "In diesen Absatz einfügen",

  // table block
  tableHeaderCell: "Spaltenüberschrift",
  tableCell: "Zelle",
  tableAddRow: "Zeile hinzufügen",
  tableAddColumn: "Spalte hinzufügen",
  tableRemoveRow: "Zeile entfernen",
  tableRemoveColumn: "Spalte entfernen",
  tableBlockLabel: "Bearbeitbare Tabelle",

  // code block
  codeSearchLanguage: "Sprache suchen…",
  codeNoLanguage: "Keine passende Sprache",
  codeCopy: "Kopieren",
  codeCopied: "Kopiert",
  codeInputLabel: "Code",
  codePlaceholder: "Code einfügen oder eintippen…",
  codeWrap: "Zeilenumbruch",

  // cross-reference chips + picker
  refSection: "Abschnitt",
  refEquation: "Gl.",
  refTable: "Tabelle",
  refFigure: "Abbildung",
  refBroken: "defekter Verweis",
  refInsert: "Querverweis einfügen",
  refInsertTitle: "Querverweis einfügen",
  refClose: "Schließen",
  refNoneOfKind: "Noch nichts von dieser Art.",
  refTabEquations: "Formeln",
  refTabSections: "Abschnitte",
  refTabTables: "Tabellen",
  refTabFigures: "Abbildungen",

  // Drive (ADR 0027) + Spaces (ADR 0026)
  close: "Schließen",
  driveMyFiles: "Meine Dateien",
  driveSpaces: "Spaces",
  driveLocations: "Drive-Orte",
  driveTrash: "Papierkorb",
  driveNewFolder: "Neuer Ordner",
  driveNew: "Neu",
  driveKindDoc: "Dokument",
  driveKindSheet: "Sheet",
  driveKindWord: "Word-Dokument",
  driveKindExcel: "Excel-Tabelle",
  driveKindSlides: "Präsentation (PowerPoint)",
  driveKindFolder: "Ordner",
  driveNameNew: (kind: string): string => `${kind} benennen`,
  driveNewSpace: "Neuer Space",
  driveNewSpacePrompt: "Namen für den neuen Space eingeben",
  driveUpload: "Hochladen",
  driveUploading: "Wird hochgeladen…",
  driveLoadingFile: (name: string) => `${name} wird geöffnet…`,
  driveOpeningEditor: "Ihre Datei",
  driveFileOpenFailedTitle: "Diese Datei wurde nicht geöffnet",
  driveFileUnavailable:
    "Sie wurde möglicherweise verschoben oder gelöscht. Kehren Sie zu Ihren Dateien zurück und wählen Sie ein anderes Element.",
  driveEditorLoadFailed: (reason: string) =>
    `Drive konnte diese Datei nicht öffnen. ${reason}`,
  driveBackToFiles: "Zurück zu den Dateien",
  driveLoading: "Ihre Dateien werden geladen…",
  driveRetry: "Erneut versuchen",
  driveUnknownError: "Der Server hat keinen Grund angegeben.",
  driveLoadFailedTitle: "Ihre Dateien wurden nicht geladen",
  driveLoadFailed: (reason: string): string =>
    `Versuchen Sie es erneut. Server: ${reason}`,
  driveActionFailed: (action: string, reason: string): string =>
    `${action} wurde nicht abgeschlossen. Versuchen Sie es erneut. Server: ${reason}`,
  driveMovedToTrash: (name: string): string =>
    `${name} in den Papierkorb verschoben.`,
  driveRestoredFromTrash: (name: string): string =>
    `${name} wiederhergestellt.`,
  driveUndo: "Rückgängig",
  driveSelected: (count: number): string =>
    count === 1 ? "1 Element ausgewählt" : `${count} Elemente ausgewählt`,
  driveSelectItem: (name: string): string => `${name} auswählen`,
  driveSelectAll: "Alle sichtbaren Elemente auswählen",
  driveClearSelection: "Auswahl aufheben",
  driveSelectionActions: "Aktionen für die Auswahl",
  driveItemsMovedToTrash: (count: number): string =>
    `${count} Elemente in den Papierkorb verschoben.`,
  driveItemsRestored: (count: number): string =>
    `${count} Elemente wiederhergestellt.`,
  drivePurgeManyConfirm: (count: number): string =>
    `${count} Elemente endgültig löschen? Das kann nicht rückgängig gemacht werden.`,
  driveVersionsLoadFailed: (reason: string): string =>
    `Der Versionsverlauf wurde nicht geladen. Versuchen Sie es erneut. Server: ${reason}`,
  driveMembersLoadFailed: (reason: string): string =>
    `Die Mitglieder wurden nicht geladen. Versuchen Sie es erneut. Server: ${reason}`,
  driveMembers: "Mitglieder",
  driveActions: "Aktionen",
  driveEmpty:
    "Dieser Ordner ist leer. Laden Sie eine Datei hoch oder erstellen Sie einen Ordner.",
  driveEmptyTitle: "Noch nichts hier",
  driveEmptyReadOnly: "Dieser Space enthält noch keine Dateien.",
  driveEmptyTrashTitle: "Der Papierkorb ist leer",
  driveFolderEmpty: "Dieser Ordner ist leer",
  driveUploadHere: "Hierher hochladen",
  driveFolderLoading: (name: string): string => `${name} wird geladen…`,
  driveFolderLoadFailed: (reason: string): string =>
    `Dieser Ordner wurde nicht geladen. Server: ${reason}`,
  driveSpacesLoadFailed: (reason: string): string =>
    `Ihre Spaces wurden nicht geladen. Versuchen Sie es erneut. Server: ${reason}`,
  driveSort: "Sortieren",
  driveSortNameAsc: "Name (A–Z)",
  driveSortNameDesc: "Name (Z–A)",
  driveSortNewest: "Neueste zuerst",
  driveSortOldest: "Älteste zuerst",
  driveSortLargest: "Größte zuerst",
  driveSortSmallest: "Kleinste zuerst",
  driveView: "Ansicht",
  driveViewExtraLarge: "Extra große Symbole",
  driveViewLarge: "Große Symbole",
  driveViewMedium: "Mittelgroße Symbole",
  driveViewSmall: "Kleine Symbole",
  driveViewList: "Liste",
  driveViewDetails: "Details",
  driveViewTiles: "Kacheln",
  driveViewContent: "Inhalt",
  driveViewNavigationPane: "Navigationsbereich",
  driveViewCompact: "Kompakte Ansicht",
  driveViewExtensions: "Dateinamenerweiterungen",
  driveEmptyTrash: "Der Papierkorb ist leer.",
  driveColName: "Name",
  driveColSize: "Größe",
  driveColModified: "Geändert",
  driveDetailsTitle: "Details",
  driveDetailsShow: (name: string): string => `Details zu ${name}`,
  driveOpen: "Öffnen",
  driveDownload: "Herunterladen",
  driveRename: "Umbenennen",
  driveMove: "Verschieben",
  driveCopy: "Kopie erstellen",
  driveVersionHistory: "Versionsverlauf",
  driveTrashAction: "In den Papierkorb",
  driveRestore: "Wiederherstellen",
  driveDeleteForever: "Endgültig löschen",
  driveNewFolderPrompt: "Namen für den neuen Ordner eingeben",
  driveRenamePrompt: "Neuer Name",
  driveTrashConfirm: (name: string) =>
    `„${name}“ in den Papierkorb verschieben?`,
  drivePurgeConfirm: (name: string) =>
    `„${name}“ endgültig löschen? Das kann nicht rückgängig gemacht werden.`,
  driveMoveTo: "Verschieben nach…",
  driveCopyTo: "Kopieren nach…",
  driveDestHint: "Das Element übernimmt die Zugriffsrechte des Zielorts.",
  driveNoVersions: "Keine früheren Versionen.",
  driveCurrent: "Aktuell",
  driveMembersOf: (name: string) => `Mitglieder von ${name}`,
  driveRole: (role: string): string =>
    role === "manager"
      ? "Manager"
      : role === "editor"
        ? "Bearbeiter"
        : "Betrachter",
  driveAddMemberPlaceholder: "Person per E-Mail hinzufügen",
  driveAddMemberLabel: "E-Mail-Adresse",
  driveMemberRoleLabel: "Rolle",
  driveAdd: "Hinzufügen",
  driveRemoveMember: "Entfernen",
  driveRemoveMemberFor: (who: string): string => `${who} entfernen`,
  driveRemoveMemberConfirm: (who: string) =>
    `${who} aus diesem Space entfernen?`,
  driveMemberError:
    "Diese Person konnte nicht hinzugefügt werden — prüfen Sie die E-Mail-Adresse und Ihre Rolle.",
  driveNewDoc: "Neues Doc",
  driveCreateDocument: "Neues Dokument",
  driveAloDocument: "Alo-Dokument",
  driveCreateMore: "Weitere Erstelloptionen",
  driveNewDocPrompt: "Namen für das neue Doc eingeben",
  driveNewSheetPrompt: "Namen für das neue Sheet eingeben",
  driveImporting: (name: string): string => `${name} wird importiert…`,
  driveImportNote:
    "Wir öffnen die Datei als alo Sheet. Die Formatierung kann leicht abweichen — Ihre Originaldatei bleibt unverändert in Drive.",
  driveImportFailed: (name: string): string =>
    `${name} konnte nicht importiert werden. Sie können das Original weiterhin herunterladen.`,
  driveNewBase: "Neue Base",
  driveNewBasePrompt: "Namen für die neue Base eingeben",

  // Sheets (the spreadsheet editor)
  sheetDownloadXlsx: "Als Excel herunterladen (.xlsx)",
  sheetDownloadXlsxShort: "Excel",
  sheetName: "Name des Sheets",
  sheetLoading: "Ihr Sheet wird geladen…",
  sheetLoadFailedTitle: "Dieses Sheet wurde nicht geladen",
  docLoading: "Ihr Dokument wird geladen…",
  docLoadFailedTitle: "Dieses Dokument wurde nicht geladen",
  docSaveFailed: (reason: string): string =>
    `Ihre letzten Änderungen sind noch nicht gespeichert. Wählen Sie „Erneut versuchen“, um sie zu speichern. Server: ${reason}`,
  sheetSaveFailed: (reason: string): string =>
    `Ihre letzten Änderungen sind noch nicht gespeichert. Wir versuchen es weiter. Server: ${reason}`,
  sheetSaved: "Gespeichert",
  sheetExport: "Exportieren",
  sheetMore: "Weitere Aktionen",
  sheetRibbon: "Formatierung",
  sheetTabHome: "Start",
  sheetTabOthers: "Weitere",
  sheetTabInsert: "Einfügen",
  sheetTabDraw: "Zeichnen",
  sheetTabLayout: "Seitenlayout",
  sheetTabFormulas: "Formeln",
  sheetTabData: "Daten",
  sheetTabReview: "Überprüfen",
  sheetTabView: "Ansicht",
  sheetTabSoon: (name: string): string =>
    `Die Werkzeuge unter „${name}“ folgen in Kürze.`,
  sheetGroupCellSize: "Zellengröße",
  sheetRowHeight: "Zeilenhöhe",
  sheetColumnWidth: "Spaltenbreite",
  sheetAutoFitRow: "Zeilenhöhe automatisch anpassen",
  sheetAutoFitColumn: "Spaltenbreite automatisch anpassen",
  sheetGroupVisibility: "Sichtbarkeit",
  sheetHideRow: "Ausgewählte Zeile ausblenden",
  sheetShowRows: "Alle Zeilen einblenden",
  sheetHideColumn: "Ausgewählte Spalte ausblenden",
  sheetShowColumns: "Alle Spalten einblenden",
  sheetGroupSheetOptions: "Sheet-Optionen",
  sheetToggleGridlines: "Gitternetzlinien",
  sheetGridlineColor: "Farbe der Gitternetzlinien",
  sheetGroupDirection: "Schreibrichtung",
  sheetLeftToRight: "Von links nach rechts",
  sheetRightToLeft: "Von rechts nach links",
  sheetUndo: "Rückgängig",
  sheetRedo: "Wiederholen",
  sheetGroupHistory: "Rückgängig",
  sheetGroupFont: "Schriftart",
  sheetGroupBorders: "Rahmen",
  sheetGroupRotation: "Drehung",
  sheetGroupAlignment: "Ausrichtung",
  sheetGroupWrap: "Umbruch",
  sheetGroupMerge: "Verbinden",
  sheetWrapOverflow: "Überlauf",
  sheetWrapText: "Umbrechen",
  sheetWrapClip: "Abschneiden",
  sheetMergeAll: "Alle verbinden",
  sheetMergeAcross: "Zeilenweise verbinden",
  sheetMergeVertically: "Vertikal verbinden",
  sheetUnmerge: "Verbindung aufheben",
  sheetGroupNumber: "Zahl",
  sheetFontFamily: "Schriftart",
  sheetFontSize: "Schriftgröße",
  sheetBold: "Fett",
  sheetItalic: "Kursiv",
  sheetUnderline: "Unterstrichen",
  sheetStrike: "Durchgestrichen",
  sheetAlignLeft: "Linksbündig",
  sheetAlignCenter: "Zentriert",
  sheetAlignRight: "Rechtsbündig",
  sheetMerge: "Zellen verbinden",
  sheetNumberFormat: "Zahlenformat",
  sheetCellStyles: "Zellenformatvorlagen",
  sheetMoreStyles: "Weitere Zellenformatvorlagen",
  sheetStyleDefault: "Standard",
  sheetStyleHeading1: "Überschrift 1",
  sheetStyleHeading2: "Überschrift 2",
  sheetStyleHeading3: "Überschrift 3",
  sheetStyleHeading4: "Überschrift 4",
  sheetStyleTitle: "Titel",
  sheetStyleSubtitle: "Untertitel",
  sheetFormatGeneral: "Standard",
  sheetFormatNumber: "Zahl",
  sheetFormatCurrency: "Währung",
  sheetFormatPercentage: "Prozent",
  sheetFormatDate: "Datum",
  sheetFormatText: "Text",
  sheetFormatPreviewGeneral: "1234,56",
  sheetFormatPreviewNumber: "1.234,56",
  sheetFormatPreviewCurrency: "1.234,56 €",
  sheetFormatPreviewPercentage: "12,34 %",
  sheetFormatPreviewDate: "06.08.2026",
  sheetFormatPreviewText: "Text",
  sheetFontGrow: "Schrift vergrößern",
  sheetFontShrink: "Schrift verkleinern",
  sheetFontColor: "Textfarbe",
  sheetFillColor: "Füllfarbe",
  sheetAlignTop: "Oben ausrichten",
  sheetAlignMiddle: "Mittig ausrichten",
  sheetAlignBottom: "Unten ausrichten",
  sheetWrap: "Text umbrechen",
  sheetGroupCells: "Zellen",
  sheetInsert: "Einfügen",
  sheetDelete: "Löschen",
  sheetFormat: "Format",
  sheetMoreCellOptions: "Weitere Zellenoptionen",
  sheetSortFilter: "Sortieren & Filtern",
  sheetGroupClear: "Löschen",
  sheetGroupRows: "Zeilen",
  sheetGroupColumns: "Spalten",
  sheetGroupView: "Fenster",
  sheetInsertRowAbove: "Zeile oberhalb einfügen",
  sheetInsertRowBelow: "Zeile unterhalb einfügen",
  sheetInsertColLeft: "Spalte links einfügen",
  sheetInsertColRight: "Spalte rechts einfügen",
  sheetDeleteRow: "Zeile löschen",
  sheetDeleteColumn: "Spalte löschen",
  sheetClearContents: "Inhalte löschen",
  sheetClearFormats: "Formatierung löschen",
  sheetFreeze: "Fenster fixieren",
  sheetUnfreeze: "Fixierung aufheben",
  sheetGroupClipboard: "Zwischenablage",
  sheetGroupStyles: "Formatvorlagen",
  sheetGroupEditing: "Bearbeiten",
  sheetGroupSortFilter: "Sortieren & Filtern",
  sheetGroupDataTools: "Datentools",
  sheetGroupCharts: "Diagramme",
  sheetChartBar: "Balkendiagramm",
  sheetChartLine: "Liniendiagramm",
  sheetChartPie: "Kreisdiagramm",
  sheetCharts: "Diagramme in diesem Sheet",
  sheetChartRemove: "Diagramm entfernen",
  sheetChartSelectionHint:
    "Wählen Sie eine Kopfzeile, eine Kategoriespalte und mindestens eine Zahlenreihe aus.",
  sheetChartExcelLimit:
    "Diagramme bleiben im alo Sheet live. Der Excel-Export enthält derzeit die Zellen, aber nicht diese Diagramme.",
  sheetChartSeries: (number: number) => `Reihe ${number}`,
  chartTabMissing:
    "Das von diesem Diagramm verwendete Tabellenblatt existiert nicht mehr.",
  chartRangesRagged: "Die Diagrammbereiche sind nicht mehr gleich lang.",
  chartTooLarge:
    "Diese Diagrammauswahl ist zu groß, um sie sicher zu zeichnen.",
  sheetGroupProtection: "Schutz",
  sheetGroupFreeze: "Fenster fixieren",
  sheetGroupZoom: "Zoom",
  sheetGroupInsertObjects: "Objekte",
  sheetGroupDrawing: "Zeichnung",
  sheetGroupNotes: "Notizen",
  sheetGroupComments: "Kommentare",
  sheetGroupFunctionLibrary: "Funktionsbibliothek",
  sheetGroupMoreFunctions: "Weitere Funktionen",
  sheetAutoSum: "AutoSumme",
  sheetAverage: "Mittelwert",
  sheetCount: "Anzahl",
  sheetMinimum: "Minimum",
  sheetMaximum: "Maximum",
  sheetMoreFunctions: "Funktionen durchsuchen",
  sheetGroupFunctionCategories: "Funktionskategorien",
  sheetFormulaFinancial: "Finanzmathematik",
  sheetFormulaDateTime: "Datum & Uhrzeit",
  sheetFormulaMathTrig: "Mathematik & Trigonometrie",
  sheetFormulaStatistical: "Statistik",
  sheetFormulaLookup: "Nachschlagen & Verweisen",
  sheetFormulaDatabase: "Datenbank",
  sheetFormulaText: "Text",
  sheetFormulaLogical: "Logik",
  sheetFormulaInformation: "Information",
  sheetFormulaEngineering: "Technik",
  sheetFormulaCube: "Cube",
  sheetFormulaCompatibility: "Kompatibilität",
  sheetFormulaWeb: "Web",
  sheetFormulaArray: "Matrix",
  sheetDataValidation: "Datenüberprüfung",
  sheetConditionalFormatting: "Bedingte Formatierung",
  sheetTextToColumns: "Text in Spalten",
  sheetNamedRanges: "Benannte Bereiche",
  sheetProtectRange: "Bereich schützen",
  sheetUnprotectRange: "Bereichsschutz aufheben",
  sheetProtectSheet: "Blatt schützen",
  sheetUnprotectSheet: "Blattschutz aufheben",
  sheetProtectedRangeName: "Geschützter Bereich",
  sheetProtectedSheetName: "Geschütztes Blatt",
  sheetFreezeTopRow: "Oberste Zeile fixieren",
  sheetFreezeFirstColumn: "Erste Spalte fixieren",
  sheetZoomOut: "Verkleinern",
  sheetZoomReset: "100 %",
  sheetZoomIn: "Vergrößern",
  sheetInsertTable: "Tabelle",
  sheetInsertLink: "Link",
  sheetInsertImage: "Bild",
  sheetDrawingPanel: "Bilder und Zeichnung",
  sheetNote: "Notiz hinzufügen oder bearbeiten",
  sheetAddComment: "Neuer Kommentar",
  sheetCommentsPanel: "Kommentarbereich",
  sheetPaste: "Einfügen",
  sheetCut: "Ausschneiden",
  sheetCopy: "Kopieren",
  sheetPercent: "Prozent",
  sheetCurrency: "Währung",
  sheetComma: "Tausendertrennzeichen",
  sheetSortAsc: "Sortieren A → Z",
  sheetSortDesc: "Sortieren Z → A",
  sheetFilter: "Filter umschalten",
  sheetFindReplace: "Suchen & Ersetzen",
  sheetBorders: "Rahmen",
  sheetBordersAll: "Alle Rahmenlinien",
  sheetBordersOuter: "Äußerer Rahmen",
  sheetBordersInside: "Innere Rahmenlinien",
  sheetBordersTop: "Rahmenlinie oben",
  sheetBordersBottom: "Rahmenlinie unten",
  sheetBordersLeft: "Rahmenlinie links",
  sheetBordersRight: "Rahmenlinie rechts",
  sheetBordersHorizontal: "Horizontale Rahmenlinien",
  sheetBordersVertical: "Vertikale Rahmenlinien",
  sheetBordersNone: "Kein Rahmen",
  sheetBordersAdvanced: "Diagonale Rahmenlinien",
  sheetBordersDiagonalDown: "Diagonale Rahmenlinie abwärts",
  sheetBordersDiagonalUp: "Diagonale Rahmenlinie aufwärts",
  sheetBordersDiagonalDownCenter: "Diagonal abwärts mit Mittellinien",
  sheetBordersDiagonalDownBoth: "Diagonal abwärts mit beiden Mittellinien",
  sheetBordersDiagonalUpCenter: "Diagonal aufwärts mit Mittellinien",
  sheetRotation: "Drehung",
  sheetRotationNone: "Keine Drehung",
  sheetRotation45: "45° im Uhrzeigersinn drehen",
  sheetRotationMinus45: "45° gegen den Uhrzeigersinn drehen",
  sheetRotation90: "90° im Uhrzeigersinn drehen",
  sheetRotationMinus90: "90° gegen den Uhrzeigersinn drehen",
  sheetRotationVertical: "Vertikaler Text",

  // Office embed (Collabora)
  officeUnavailable:
    "Dieses Dokument konnte nicht zum Bearbeiten geöffnet werden. Versuchen Sie es erneut oder laden Sie es herunter.",
  officeLoading: "Der Office-Editor wird geöffnet…",
  officeDiscoveryMissing:
    "Der Office-Editor hat keine Editor-Adresse veröffentlicht.",
  officeLoadFailed: (reason: string): string =>
    `Versuchen Sie es erneut. Server: ${reason}`,

  // search overlay
  searchPlaceholder: "Dateien, Aufgaben und E-Mails durchsuchen…",
  searchHint:
    "Dateien und Aufgaben nach Namen durchsuchen, E-Mails nach Inhalt.",
  searchNoResults: "Nichts gefunden.",
  searchKind: (kind: string): string =>
    kind === "task"
      ? "Aufgabe"
      : kind === "message"
        ? "E-Mail"
        : kind === "folder"
          ? "Ordner"
          : kind === "doc"
            ? "Doc"
            : kind === "base"
              ? "Base"
              : "Datei",
  aiAskAbout: (q: string): string => `KI fragen: „${q}“`,
  aiSources: "Quellen",
  aiUnconfigured:
    "Die KI ist noch nicht eingerichtet — ein Administrator kann ein Modell hinzufügen. Das hier passt zu Ihrer Suche:",
  aiUnreachable: "Die KI war nicht erreichbar. Das hier passt zu Ihrer Suche:",
  aiComingSoon: "Der KI-Assistent kommt bald.",

  // The Drive file picker (first used by chat).
  pickerTitle: "Datei auswählen",
  pickerPlaces: "Orte",
  pickerMyDrive: "Mein Drive",
  pickerLoading: "Wird geladen…",
  pickerEmpty: "Noch nichts hier.",
  pickerLoadFailed: "Dieser Ordner konnte nicht geöffnet werden.",
  pickerAttach: "Anhängen",
  pickerNonePicked: "Keine Dateien ausgewählt",
  pickerPicked: (count: number, max: number): string =>
    `${count} von ${max} ausgewählt`,
  pickerPersonalNotice:
    "Dateien in „Mein Drive“ gehören nur Ihnen — die Personen im Gespräch können sie nicht öffnen. Nutzen Sie einen Space, um zu teilen.",
  cancel: "Abbrechen",

  // account settings (signature + org footer)
  settingsOpen: "Einstellungen",
  settingsTitle: "E-Mail-Einstellungen",
  settingsTabGeneral: "Allgemein",
  settingsTabOrg: "Organisation",
  settingsOooToggle: "Automatische Antworten senden",
  settingsSignature: "Ihre Signatur",
  settingsSignatureHint: "Wird unter die Nachrichten gesetzt, die Sie senden…",
  settingsOrgFooter: "Organisationsfußzeile",
  settingsOrgFooterHint:
    "Wird an jede ausgehende E-Mail Ihrer Organisation angehängt, nach der persönlichen Signatur.",
  settingsOrgFooterPlaceholder:
    "z. B. Firmenname, Adresse, rechtliche Hinweise…",
  settingsOutOfOffice: "Abwesenheit",
  settingsOutOfOfficeHint:
    "Antwortet automatisch einmal jedem, der Ihnen während Ihrer Abwesenheit schreibt.",
  settingsOooSubjectPlaceholder: "Betreff (optional) — z. B. Abwesenheitsnotiz",
  settingsOooMessagePlaceholder:
    "z. B. Ich bin bis Montag abwesend und antworte nach meiner Rückkehr.",
  settingsOooNeedsMessage:
    "Fügen Sie eine Nachricht hinzu, um die Abwesenheitsnotiz einzuschalten.",
  settingsOooFrom: "Erster Abwesenheitstag",
  settingsOooTo: "Letzter Abwesenheitstag",
  settingsOooDatesHint:
    "Leer lassen, um sofort zu starten und zu antworten, bis Sie es ausschalten.",
  settingsOooBadWindow:
    "Der letzte Abwesenheitstag kann nicht vor dem ersten liegen.",
  settingsSave: "Speichern",
  settingsSaved: "Gespeichert.",
  settingsSaveError: "Ihre Einstellungen konnten nicht gespeichert werden.",
  settingsLoadError: "Ihre Einstellungen konnten nicht geladen werden.",

  // alo Chat (ADR 0038)
  chatNewChannel: "Neuer Kanal",
  chatNewChannelPrompt:
    "Geben Sie ihm einen kurzen, sprechenden Namen — darüber treten andere bei.",
  chatNewChannelPlaceholder: "z. B. produkt-launch",
  chatCreate: "Erstellen",
  chatDirectMessage: "Direktnachricht",
  chatLoading: "Wird geladen…",
  chatSend: "Senden",
  chatEdited: "bearbeitet",
  chatSourceOne: "Antwort auf Basis von 1 Quelle",
  chatSourceCount: "Antwort auf Basis von {count} Quellen",
  chatSourceEmail: "E-Mail",
  chatSourceChat: "Chatnachricht",
  chatSourceEvent: "Kalendereintrag",
  chatSourceRemembered: "hier gemerkt",
  chatWithdrawn: "Diese Nachricht wurde zurückgezogen.",
  chatMessageSent: "Gesendet",
  chatMessageReadBy: (count: number) => `Gelesen von ${count}`,
  chatNoMessagesYet: "Noch keine Nachrichten — schreiben Sie die erste.",
  chatArchived: "Archiviert",
  chatReplyInThread: "Hier antworten",
  chatReplyHere: "Hier antworten",
  chatReplyPrivately: "Privat antworten",
  chatReplyingHere: "Antwort hier",
  chatReplyingPrivately: (who: string): string => `Private Antwort an ${who}`,
  chatCancelReply: "Antwort abbrechen",
  chatAddReaction: "Reaktion hinzufügen",
  chatAgentTag: "Agent",
  chatOlder: "Frühere Nachrichten anzeigen",
  chatBrowse: "Kanäle durchstöbern",
  chatNewDm: "Neue Unterhaltung",
  chatFindPerson: "Person finden",
  chatFindPersonHint: "Geben Sie mindestens zwei Buchstaben der Adresse ein.",
  chatNobodyFound: "Dazu passt hier niemand.",
  chatPeopleFailed: "Die Suche konnte nicht ausgeführt werden.",
  chatDmFailed: "Die Unterhaltung konnte nicht gestartet werden.",
  chatJoin: "Beitreten",
  chatJoined: "Öffnen",
  chatNothingToJoin: "Noch keine öffentlichen Kanäle in diesem Arbeitsbereich.",
  chatBrowseFailed: "Die Kanäle konnten nicht aufgelistet werden.",
  chatJoinFailed: "Dem Kanal konnte nicht beigetreten werden.",
  chatEditAction: "Bearbeiten",
  chatWithdrawAction: "Zurückziehen",
  chatEditLabel: "Diese Nachricht bearbeiten",
  chatEditSave: "Speichern",
  chatEditCancel: "Abbrechen",
  chatEditFailed: "Die Änderung konnte nicht gespeichert werden.",
  chatWithdrawFailed: "Die Nachricht konnte nicht zurückgezogen werden.",
  chatWhoIsHere: "Wer ist hier",
  chatMembersAndAgents: "Mitglieder & Agenten",
  chatThinking: (handle: string): string => `@${handle} denkt nach`,
  chatStop: "Stopp",
  chatBold: "Fett",
  chatItalic: "Kursiv",
  chatInlineCode: "Code",
  chatCodeBlock: "Codeblock",
  chatCodeBlockHint: "Fügt einen formatierten Block für Code oder Befehle ein.",
  chatFormulaHint: "Fügt eine mathematische Formel ein.",
  chatFormatting: "Textformatierung",
  chatFormula: "Formel",
  chatBulletList: "Aufzählung",
  chatQuoteAction: "Zitieren",
  chatFormatHint: "Text",
  chatMeetingPreview: "Hat ein Meeting gestartet",
  chatBackToList: "Zurück zu den Unterhaltungen",
  chatJumpTo: "Zu einer Unterhaltung springen",
  chatNoRoom: "Dazu passt keine Unterhaltung.",
  chatDropFiles: "Loslassen, um von Ihrem Computer zu teilen",
  chatNewMessages: "Neue Nachrichten",
  chatToday: "Heute",
  chatYesterday: "Gestern",
  chatBeginning: (name: string): string => `Hier beginnt ${name}`,
  chatBeginningDm: "Hier beginnt Ihre Unterhaltung",
  chatSectionChannels: "Kanäle",
  chatFilterAll: "Alle",
  chatFilterUnread: "Ungelesen",
  chatFilterThreads: "Threads",
  chatFilterMentions: "Erwähnungen",
  chatCompose: "Schreiben",
  chatSectionDirect: "Direktnachrichten",
  chatSectionArchived: "Archiviert",
  chatChannelActions: (name: string): string => `Aktionen für ${name}`,
  chatRename: "Kanal umbenennen",
  chatRenamePrompt: "Alle im Kanal sehen den neuen Namen.",
  chatRenameSave: "Umbenennen",
  chatRenameFailed: "Der Kanal konnte nicht umbenannt werden.",
  chatAddDescription: "Beschreibung hinzufügen",
  chatEditDescription: "Beschreibung bearbeiten",
  chatDescriptionPrompt:
    "Helfen Sie anderen zu verstehen, wofür dieser Kanal da ist.",
  chatDescriptionSave: "Beschreibung speichern",
  chatDescriptionFailed:
    "Die Kanalbeschreibung konnte nicht gespeichert werden.",
  chatArchiveAction: "Kanal archivieren",
  chatArchiveTitle: (name: string): string => `${name} archivieren?`,
  chatArchiveWarning:
    "Nichts wird gelöscht. Der Verlauf bleibt lesbar, aber niemand kann hier mehr schreiben.",
  chatArchiveConfirm: "Archivieren",
  chatArchiveFailed: "Der Kanal konnte nicht archiviert werden.",
  chatClose: "Schließen",
  chatOwner: "Inhaber",
  chatAgentsHere: "Agenten in dieser Unterhaltung",
  chatAgentNothingYet: "Wurde noch nichts gefragt",
  chatAgentRecord: (answers: number, actions: number): string => {
    const said = answers === 1 ? "1 Antwort" : `${answers} Antworten`;
    if (actions === 0) return said;
    return `${said} · ${actions === 1 ? "1 Aktion" : `${actions} Aktionen`} genehmigt`;
  },
  chatAgentsAvailable: "Zum Hinzufügen verfügbar",
  chatNoAgentsHere:
    "Noch keine Agenten hier. Fügen Sie einen hinzu und erwähnen Sie ihn mit Namen.",
  chatPeopleHere: "Personen",
  chatAgentAdd: (handle: string): string =>
    `@${handle} zu dieser Unterhaltung hinzufügen`,
  chatAgentRemove: (handle: string): string => `@${handle} entfernen`,
  chatAgentAddFailed: "Der Agent konnte nicht hinzugefügt werden.",
  chatAgentRemoveFailed: "Der Agent konnte nicht entfernt werden.",
  agentMemoryTitle: (handle: string): string => `Was @${handle} sich merkt`,
  agentMemoryShared:
    "In dieser Unterhaltung gelernt. Alle hier können diese Liste lesen.",
  agentMemoryAboutYou:
    "Was der Agent sich aus diesem Einzelgespräch über Sie merkt. Nur Sie sehen diese Liste.",
  agentMemoryEmpty:
    "Noch ist nichts gemerkt. Was der Agent hier lernt — und was Sie ihn ausdrücklich bitten sich zu merken — erscheint in dieser Liste.",
  agentMemoryExplicit: "Direkt mitgeteilt",
  agentMemoryFromTurn: "Aus einer Antwort gelernt",
  agentMemoryForget: "Vergessen",
  agentMemoryForgetFact: (fact: string): string => `„${fact}“ vergessen`,
  agentMemoryLoadFailed: "Die gemerkten Fakten konnten nicht geladen werden.",
  agentMemoryForgetFailed: "Das konnte nicht vergessen werden.",
  agentInstructionsTitle: "Daueranweisungen",
  agentInstructionsIntro:
    "Einmal gebeten, im Voraus. Jede Anweisung läuft im Namen der Person, die darum gebeten hat, und alle hier können diese Liste lesen.",
  agentInstructionsEmpty:
    "Noch nichts eingerichtet. Wählen Sie einen Agenten, sagen Sie, was er tun soll, und wie oft — er führt Ihre Worte nach diesem Rhythmus aus und schreibt hier.",
  agentInstructionHourly: "Läuft jede Stunde",
  agentInstructionDaily: "Läuft jeden Tag",
  agentInstructionWeekly: "Läuft jede Woche",
  agentInstructionEveryHours: (hours: number): string =>
    `Läuft alle ${hours} Stunden`,
  agentInstructionEveryMinutes: (minutes: number): string =>
    `Läuft alle ${minutes} Minuten`,
  agentInstructionOnEvent: (verb: string): string =>
    `Läuft nach jedem „${verb}“`,
  agentInstructionNextRun: (at: string): string => `Nächster Lauf ${at}`,
  agentInstructionAskedBy: (who: string): string => `Eingerichtet von ${who}`,
  agentInstructionPaused:
    "Pausiert — die Person, die darum gebeten hat, hat den Raum verlassen.",
  agentInstructionCancel: "Beenden",
  agentInstructionCancelThis: (text: string): string => `„${text}“ beenden`,
  agentInstructionAgentLabel: "Agent",
  agentInstructionTextLabel: "Was soll er tun?",
  agentInstructionTextPlaceholder:
    "z. B. die überfälligen Rechnungen auflisten",
  agentInstructionScheduleLabel: "Wie oft",
  agentInstructionOptionHourly: "Jede Stunde",
  agentInstructionOption4Hours: "Alle 4 Stunden",
  agentInstructionOptionDaily: "Jeden Tag",
  agentInstructionOptionWeekly: "Jede Woche",
  agentInstructionAdd: "Anweisung hinzufügen",
  agentInstructionsLoadFailed:
    "Die Daueranweisungen konnten nicht geladen werden.",
  agentInstructionCreateFailed:
    "Die Anweisung konnte nicht hinzugefügt werden.",
  agentInstructionCancelFailed: "Das konnte nicht abgebrochen werden.",
  recordAgentTitle: "Der Agent dieses Eintrags",
  recordAgentOriginNone: "Dieser Eintrag sagt nicht, woher er stammt.",
  recordAgentOriginPerson: (who: string): string => `Erstellt von ${who}.`,
  recordAgentOriginThread: (room: string): string =>
    `Festgehalten aus der Unterhaltung „${room}“.`,
  recordAgentOriginThreadUnnamed: "Festgehalten aus einer Unterhaltung.",
  recordAgentOriginEmail: "Entstanden aus einer E-Mail.",
  recordAgentOriginEvent: "Aus einem Kalendertermin.",
  recordAgentOriginQuote: (quote: string): string =>
    `Entstanden aus Angebot ${quote}.`,
  recordAgentOriginFrom: (source: string): string => `Aus ${source}.`,
  recordAgentOpenSource: "Öffnen",
  recordAgentCanDo: (handle: string): string => `Was @${handle} hier tun kann`,
  recordAgentAskPlaceholder: (handle: string): string =>
    `@${handle} dazu fragen…`,
  recordAgentAsk: "Fragen",
  recordAgentAsking: (handle: string): string =>
    `Frage an @${handle} gestellt…`,
  recordAgentNoAnswerYet:
    "Noch keine Antwort — sie erscheint in der Unterhaltung.",
  recordAgentOpenConversation: "Unterhaltung öffnen",
  recordAgentAskFailed: "Die Frage konnte nicht gestellt werden.",
  recordAgentVerbFailed: "Das konnte nicht gestartet werden.",
  recordAgentAskAbout: (record: string, question: string): string =>
    `Zu „${record}“: ${question}`,
  recordAgentVerbChaseTask: "Nachhaken",
  recordAgentVerbSetTaskPriority: "Priorität setzen",
  recordAgentVerbCompleteTask: "Als erledigt markieren",
  recordAgentVerbReassignTask: "Übergeben",
  recordAgentDraftChaseTask: (task: string): string =>
    `Hake bei „${task}“ nach.`,
  recordAgentDraftSetTaskPriority: (task: string): string =>
    `Setze die Priorität von „${task}“ auf `,
  recordAgentDraftCompleteTask: (task: string): string =>
    `Markiere „${task}“ als erledigt.`,
  recordAgentDraftReassignTask: (task: string): string =>
    `Übertrage „${task}“ an `,
  recordAgentVerbMoveDealStage: "Phase ändern",
  recordAgentVerbDraftFollowup: "Nachfassnachricht entwerfen",
  recordAgentDraftMoveDealStage: (deal: string): string =>
    `Verschiebe „${deal}“ in die Phase `,
  recordAgentDraftDraftFollowup: (deal: string): string =>
    `Entwirf eine Nachfassnachricht für „${deal}“.`,
  recordAgentVerbApproveExpense: "Genehmigen",
  recordAgentVerbSuggestCategories: "Kategorien vorschlagen",
  recordAgentDraftApproveExpense: (merchant: string): string =>
    `Genehmige die Spesenabrechnung „${merchant}“.`,
  recordAgentDraftSuggestCategories:
    "Gehe meine Spesen ohne Kategorie durch und schlage für jede eine vor.",
  recordAgentVerbProjectStatus: "Status zusammenfassen",
  recordAgentVerbLogTime: "Zeit darauf erfassen",
  recordAgentVerbDraftTimesheet: "Aus meinem Kalender entwerfen",
  recordAgentDraftProjectStatus: (project: string): string =>
    `Fasse den Status von „${project}“ zusammen.`,
  recordAgentDraftLogTime: (project: string): string =>
    `Erfasse Zeit auf „${project}“: `,
  recordAgentDraftDraftTimesheet: (week: string): string =>
    `Entwirf meinen Stundenzettel für ${week} aus meinem Kalender.`,
  recordAgentVerbReceiveDelivery: "Lieferung annehmen",
  recordAgentDraftReceiveDelivery: (order: string): string =>
    `Nimm die Lieferung zu „${order}“ an.`,
  recordAgentVerbApproveLeave: "Genehmigen",
  recordAgentVerbDraftLetter: "Ein Schreiben entwerfen",
  recordAgentDraftApproveLeave: (person: string): string =>
    `Genehmige den Urlaubsantrag von „${person}“.`,
  recordAgentDraftDraftLetter: (person: string): string =>
    `Entwirf ein Schreiben für „${person}“ aus einer Vorlage.`,
  recordAgentOriginImport: (format: string): string =>
    `Importiert aus einer ${format}-Datei.`,
  recordAgentPanelToggle: "Sein Agent",
  recordAgentFocusRecord: (record: string): string =>
    `Der Agent von „${record}“`,
  recordAgentVerbRenameFile: "Umbenennen",
  recordAgentDraftRenameFile: (file: string): string =>
    `Benenne „${file}“ um in `,
  recordAgentVerbMoveFile: "Verschieben",
  recordAgentDraftMoveFile: (file: string): string =>
    `Verschiebe „${file}“ in den Ordner `,
  recordAgentVerbListFolder: "Inhalt auflisten",
  recordAgentDraftListFolder: (folder: string): string =>
    `Was ist im Ordner „${folder}“?`,
  recordAgentVerbDraftSection: "Einen Abschnitt entwerfen",
  recordAgentDraftDraftSection: (document: string): string =>
    `Entwirf einen Abschnitt für „${document}“ über `,
  recordAgentVerbRewriteDoc: "Eine Passage umschreiben",
  recordAgentDraftRewriteDoc: (document: string): string =>
    `Schreibe in „${document}“ die Passage über `,
  recordAgentVerbWriteFormula: "Eine Formel schreiben",
  recordAgentDraftWriteFormula: (sheet: string): string =>
    `Schreibe in „${sheet}“ eine Formel, die `,
  recordAgentVerbTidyColumn: "Eine Spalte bereinigen",
  recordAgentDraftTidyColumn: (sheet: string): string =>
    `Bereinige in „${sheet}“ die Spalte `,
  recordAgentVerbMeetingPrep: "Darauf vorbereiten",
  recordAgentDraftMeetingPrep: (meeting: string): string =>
    `Was brauche ich für „${meeting}“?`,
  recordAgentVerbRescheduleEvent: "Verschieben",
  recordAgentDraftRescheduleEvent: (meeting: string): string =>
    `Verschiebe „${meeting}“ auf `,
  recordAgentVerbCancelEvent: "Absagen",
  recordAgentDraftCancelEvent: (meeting: string): string =>
    `Sage „${meeting}“ ab.`,
  recordAgentOriginSender: (who: string): string => `Gesendet von ${who}.`,
  recordAgentVerbCatchUpRoom: "Bring mich auf den Stand",
  recordAgentDraftCatchUpRoom: (room: string): string =>
    `Bring mich zu „${room}“ auf den Stand.`,
  recordAgentVerbFindInRoom: "Darin etwas suchen",
  recordAgentDraftFindInRoom: (room: string): string =>
    `Suche in „${room}“ nach `,
  recordAgentVerbMeetingRecord: "Was darin gesagt wurde",
  recordAgentDraftMeetingRecord: (meeting: string): string =>
    `Was ist in „${meeting}“ passiert?`,
  recordAgentVerbMeetingMinutes: "Protokoll schreiben",
  recordAgentDraftMeetingMinutes: (meeting: string): string =>
    `Schreibe das Protokoll von „${meeting}“.`,
  recordAgentVerbInsightChange: "Was sich verändert hat",
  recordAgentDraftInsightChange: (chart: string): string =>
    `Wie hat sich „${chart}“ seit der Periode davor verändert?`,
  recordAgentVerbPinChart: "Ein Diagramm anheften",
  recordAgentDraftPinChart: (board: string): string =>
    `Hefte an die Tafel „${board}“ ein Diagramm, das `,
  recordAgentVerbDraftReply: "Antwort entwerfen",
  recordAgentDraftDraftReply: (subject: string): string =>
    `Entwirf eine Antwort auf „${subject}“, die `,
  recordAgentVerbThreadLookup: "Den Verlauf zusammenfassen",
  recordAgentDraftThreadLookup: (subject: string): string =>
    `Fasse das Gespräch „${subject}“ zusammen.`,
  recordAgentVerbCorrespondence: "Was wir ihnen geschrieben haben",
  recordAgentDraftCorrespondence: (person: string): string =>
    `Was haben wir ${person} geschrieben?`,
  recordAgentVerbWriteToThem: "Ihnen schreiben",
  recordAgentDraftWriteToThem: (person: string): string =>
    `Entwirf eine E-Mail an ${person} über `,
  recordAgentVerbSiteStatus: "Wie sie dasteht",
  recordAgentDraftSiteStatus: (site: string): string =>
    `Wie steht die Website „${site}“ da?`,
  recordAgentVerbSiteSeoReview: "Für Suchmaschinen prüfen",
  recordAgentDraftSiteSeoReview: (site: string): string =>
    `Prüfe „${site}“ für Suchmaschinen.`,
  recordAgentVerbSitePublish: "Veröffentlichen",
  recordAgentDraftSitePublish: (site: string): string =>
    `Veröffentliche „${site}“.`,
  recordAgentOriginQuoteUnnamed: "Aus einem angenommenen Angebot erstellt.",
  recordAgentOriginSchedule: "Von einer wiederkehrenden Abrechnung erstellt.",
  recordAgentOriginCorrection: "Zur Korrektur einer Rechnung erstellt.",
  recordAgentVerbChaseInvoice: "Anmahnen",
  recordAgentDraftChaseInvoice: (invoice: string): string =>
    `Schreibe eine Erinnerung zur Rechnung ${invoice}.`,
  recordAgentVerbRecordPayment: "Zahlung erfassen",
  recordAgentDraftRecordPayment: (invoice: string): string =>
    `Erfasse eine Zahlung zur Rechnung ${invoice}.`,
  recordAgentVerbQuoteToInvoice: "Daraus eine Rechnung machen",
  recordAgentDraftQuoteToInvoice: (quote: string): string =>
    `Angebot ${quote} wurde angenommen — erstelle die Rechnung dazu.`,
  recordAgentVerbCustomerStanding: "Wie wir dastehen",
  recordAgentDraftCustomerStanding: (customer: string): string =>
    `Wie stehen wir bei ${customer}?`,
  recordAgentVerbCustomerUnpaid: "Was sie schulden",
  recordAgentDraftCustomerUnpaid: (customer: string): string =>
    `Was schuldet uns ${customer} noch?`,
  recordAgentVerbCustomerOpenQuotes: "Was bei ihnen offen ist",
  recordAgentDraftCustomerOpenQuotes: (customer: string): string =>
    `Was ist bei ${customer} offen?`,
  chatSearchPlaceholder: "Nachrichten, Personen, Kanäle durchsuchen…",
  chatSearchClear: "Suche leeren",
  chatSearchNothing: "Nichts gefunden.",
  chatSearchFailed: "Die Suche konnte nicht ausgeführt werden.",
  chatProposalNotYours:
    "Nur die Person, die gefragt hat, kann das genehmigen — es würde mit ihren Zugriffsrechten ausgeführt.",
  chatProposalSettled: (state: string): string =>
    state === "approved" ? "Genehmigt und erledigt." : `Status: ${state}.`,
  chatDecideFailed: "Das konnte nicht entschieden werden.",
  chatAttach: "Datei anhängen",
  chatShare: "Etwas teilen",
  chatShareFile: "Datei aus Drive",
  chatShareFileHint: "Ein Verweis, keine Kopie — sie bleibt in Drive",
  chatShareMention: "Jemanden erwähnen",
  chatShareMentionHint: "Personen und Agenten in dieser Unterhaltung",
  chatShareAsk: "alo fragen",
  chatShareAskHint: "Antworten aus Ihrem gesamten Arbeitsbereich",
  chatInsertEmoji: "Emoji",
  chatEmojiSearch: "Emoji suchen",
  chatEmojiNone: "Dazu passt kein Emoji.",
  chatUnstage: (name: string): string => `${name} entfernen`,
  chatAttachFailed: "Die Datei konnte nicht geteilt werden.",
  chatOpenFile: "In Drive öffnen",
  chatFileTrashed: "im Papierkorb von Drive",
  chatMentionsYou: (count: number): string =>
    count === 1
      ? "1 Nachricht erwähnt Sie"
      : `${count} Nachrichten erwähnen Sie`,
  chatReactFailed: "Die Reaktion konnte nicht gespeichert werden.",
  chatReplies: (count: number): string =>
    count === 1 ? "1 Antwort" : `${count} Antworten`,
  chatThread: "Thread",
  chatThreadClose: "Thread schließen",
  chatThreadEmpty: "Noch keine Antworten — schreiben Sie die erste.",
  chatThreadPlaceholder: "Antworten…",
  chatThreadFailed: "Der Thread konnte nicht geladen werden.",
  chatArchivedNote:
    "Dieser Kanal ist archiviert. Der Verlauf bleibt hier lesbar, aber Neues kann nicht mehr gesendet werden.",
  chatNoChannelsLead: "Noch keine Unterhaltungen",
  chatNoChannelsHint:
    "Erstellen Sie einen Kanal für ein Team oder ein Thema — alle darin sehen denselben Verlauf.",
  chatNoRoomOpenLead: "Wählen Sie eine Unterhaltung",
  chatNoRoomOpenHint:
    "Wählen Sie links einen Kanal oder erstellen Sie einen neuen.",
  chatComposerLabel: "Nachricht schreiben",
  chatComposerPlaceholder: (room: string): string => `Nachricht an ${room}`,
  chatLoadFailed: "Die Unterhaltungen konnten nicht geladen werden.",
  chatSendFailed:
    "Die Nachricht konnte nicht gesendet werden — Ihr Text ist noch da.",
  chatCreateFailed: "Der Kanal konnte nicht erstellt werden.",

  // alo Meet
  meetTitle: "Meeting",
  meetEyebrow: "Ihr Meeting-Bereich",
  meetSubtitle:
    "Starten Sie einen Anruf oder treten Sie einem bei, der schon läuft.",
  meetHeroTitle: "Mit einem Klick zusammen",
  meetHeroText:
    "Mikrofon an, Kamera nach Wunsch. Prüfen Sie beides, bevor jemand Sie sieht oder hört.",
  meetHappeningNow: "Läuft gerade",
  meetHappeningHint:
    "Meetings, denen Sie beitreten können, ohne nach einem Link zu fragen.",
  meetLiveCount: (count: number) =>
    count === 1 ? "1 Meeting" : `${count} Meetings`,
  meetReady: "Bereit",
  meetStartedAt: (time: string) => `Gestartet um ${time}`,
  meetStartNow: "Meeting starten",
  meetStarting: "Wird gestartet…",
  meetStartFailed:
    "Das Meeting konnte nicht gestartet werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  meetLoading: "Meetings werden geladen",
  meetLoadFailed: "Meetings konnten nicht geladen werden",
  meetLoadFailedHint:
    "Prüfen Sie Ihre Verbindung und versuchen Sie es erneut. Ein neues Meeting lässt sich weiterhin starten.",
  meetRetry: "Erneut versuchen",
  meetBack: "Zurück zu Meet",
  meetInstantTitle: "Sofort-Meeting",
  meetNothingLive: "Gerade läuft kein Meeting",
  meetWhereFrom:
    "Meetings beginnen meist dort, wo die Menschen sind — in einer Unterhaltung oder über eine Kalendereinladung. Alles, was läuft und dem Sie beitreten können, erscheint hier.",
  meetUntitled: "Sofort-Meeting",
  meetNotStarted: "Noch nicht gestartet",
  meetAddToEvent: "Meeting hinzufügen",
  meetStart: "Meeting starten",
  meetStartedHere: "hat in dieser Unterhaltung ein Meeting gestartet",
  meetJoin: "Dem Meeting beitreten",
  meetLive: "Meeting läuft",
  meetJoinNow: "Jetzt beitreten",
  meetReadyGreeting: (name: string) => (name ? `Hallo ${name}` : "Hallo"),
  meetReadyTitle: "Alles bereit zum Beitreten",
  meetReadyBody: "Prüfen Sie Kamera und Mikrofon, bevor Sie beitreten.",
  meetReadySafetyTitle: "Ihr Meeting ist geschützt",
  meetReadySafetyBody:
    "Nur eingeladene Personen und vom Gastgeber zugelassene Teilnehmende können beitreten.",
  meetSettingsAfterJoin:
    "Sie können Ihre Einstellungen auch nach dem Beitritt noch ändern.",
  meetGoodConnection: "Gute Verbindung",
  meetConnectingStatus: "Verbindung wird hergestellt",
  meetEnterFullscreen: "Vollbildmodus starten",
  meetExitFullscreen: "Vollbildmodus beenden",
  meetMicrophone: "Mikrofon",
  meetCamera: "Kamera",
  meetJoining: "Beitritt läuft…",
  meetLeave: "Verlassen",
  meetRecord: "Aufzeichnen",
  meetRecording: "Aufzeichnung",
  meetStartRecording: "Aufzeichnung starten",
  meetStopRecording: "Aufzeichnung beenden",
  meetIConsent: "Ich stimme zu",
  meetRecordingConsentTitle: "Die Aufzeichnung braucht die Zustimmung aller",
  meetRecordingConsentBody:
    "Der Gastgeber kann starten, sobald alle, die gerade im Raum sind, zustimmen.",
  meetRecordingConsentGiven: "Zustimmung erteilt",
  meetConsentCount: (count: number) =>
    count === 1 ? "1 hat zugestimmt" : `${count} haben zugestimmt`,
  meetRecordingFailed:
    "Die Aufzeichnungsaktion konnte nicht abgeschlossen werden.",
  meetGenerateMinutes: "Protokoll erstellen",
  meetMinutesTitle: "Meeting-Protokoll",
  meetMinutesActions: "Aktionspunkte",
  meetMinutesNoActions: "Keine Aktionspunkte erkannt.",
  meetMinutesFailed:
    "Für das Protokoll braucht es ein Transkript und einen konfigurierten KI-Anbieter.",
  meetPresentingTitle: "Sie präsentieren",
  meetPresentingBody:
    "Alle anderen sehen Ihren geteilten Bildschirm. Sie sehen diesen ruhigen Hinweis statt eines Spiegelkabinetts.",
  meetClose: "Schließen",
  meetJoinFailed: "Dem Meeting konnte nicht beigetreten werden.",
  meetJoinProblemTitle: "Wir konnten Sie nicht verbinden",
  meetUnavailableTitle: "Meet braucht noch eine letzte Verbindung",
  meetRaiseHand: "Hand heben",
  meetLowerHand: "Hand senken",
  meetReact: "Reaktion senden",
  meetInvite: "Einladen",
  meetInviteTitle: "Nehmen Sie an meinem alo-Meeting teil",
  meetInviteText: "Nutzen Sie diesen alo-Link, um dem Meeting beizutreten.",
  meetChatEmptyTitle: "Der Raum hört zu",
  meetChatEmptyBody:
    "Teilen Sie einen Gedanken, einen Link oder das Detail, das nach dem Anruf alle haben wollen.",
  meetChat: "Chat",
  meetCaptions: "Live-Untertitel",
  meetCaptionLanguage: "Untertitelsprache",
  meetCaptionOriginal: "Original",
  meetToolLoading: "Meeting-Werkzeuge werden geladen…",
  meetAgenda: "Agenda",
  meetAgendaHint:
    "Halten Sie den Raum darüber im Bild, was als Nächstes kommt.",
  meetAgendaPlaceholder: "Agendapunkt hinzufügen",
  meetPolls: "Umfragen",
  meetPollsHint: "Fragen Sie den Raum und sehen Sie die Antwort gemeinsam.",
  meetPollQuestion: "Frage",
  meetPollOptionOne: "Erste Option",
  meetPollOptionTwo: "Zweite Option",
  meetCreatePoll: "Umfrage erstellen",
  meetNotes: "Notizen",
  meetNotesHint: "Gemeinsame Notizen, die bei diesem Meeting bleiben.",
  meetNotesPlaceholder:
    "Entscheidungen, Kontext und nächste Schritte festhalten…",
  meetFiles: "Dateien",
  meetFilesHint: "Bilder und PDFs, die in diesem Anruf geteilt wurden.",
  meetNoFiles: "Noch keine Dateien geteilt.",
  meetToolsFailed:
    "Die Meeting-Werkzeuge wurden andernorts geändert. Laden Sie neu und versuchen Sie es erneut.",
  meetCaptionsWaiting:
    "Untertitel erscheinen, sobald der Transkriptionsdienst Sprache hört.",
  meetChatTitle: "Chat im Anruf",
  meetChatMessages: "Nachrichten",
  meetChatPeople: (count: number) => `Personen (${count})`,
  meetChatPlaceholder: "Nachricht senden",
  meetMessageSendFailed:
    "Die Nachricht wurde nicht gespeichert. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  meetEveryone: "Alle",
  meetSendTo: "An:",
  meetChooseRecipient: "Nachricht senden an",
  meetEveryoneHint: "Für alle im Meeting sichtbar",
  meetPrivateHint: "Nur diese Person erhält sie",
  meetPrivate: "Privat",
  meetReplyPrivately: "Privat antworten",
  meetMessagePrivately: "Privat schreiben",
  meetAttachFile: "Bild oder PDF hinzufügen",
  meetAddEmoji: "Emoji hinzufügen",
  meetSettings: "Meeting-Einstellungen",
  meetDeviceSettings: "Kamera und Audio",
  meetDeviceSettingsHint:
    "Änderungen wirken sofort und bleiben auf diesem Gerät.",
  meetBackgroundEffects: "Hintergrundeffekte",
  meetBackgroundEffectsHint:
    "Der Fokus bleibt auf Ihnen. Der Effekt wird auf das Video angewendet, das alle empfangen.",
  meetBackgroundNone: "Ohne",
  meetBackgroundBlur: "Weichzeichnen",
  meetBackgroundUnsupported:
    "Hintergrund-Weichzeichnen wird von diesem Browser oder dieser Kamera nicht unterstützt.",
  meetReconnecting: "Ihr Anruf wird neu verbunden",
  meetReconnectingHint:
    "Bleiben Sie hier — Ton und Video setzen automatisch wieder ein.",
  meetConnectionLost:
    "Die Verbindung des Anrufs ist abgebrochen. Treten Sie erneut bei.",
  meetPictureInPicture: "Bild im Bild",
  meetSpeaker: "Lautsprecher",
  meetDone: "Fertig",
  meetYou: "Sie",
  meetParticipant: "Teilnehmer",
  meetHost: "Gastgeber",
  meetSpeaking: "Spricht",
  meetMuted: "Stummgeschaltet",
  meetMuteParticipant: "Teilnehmer stummschalten",
  meetRemoveParticipant: "Teilnehmer entfernen",
  meetRemoveParticipantConfirm: (name: string) =>
    `${name} aus diesem Meeting entfernen?`,
  meetModerationFailed:
    "Die Aktion konnte nicht abgeschlossen werden. Versuchen Sie es erneut.",
  meetQuickReplyOne: "👍 Klingt gut",
  meetQuickReplyTwo: "Los geht’s!",
  meetQuickReplyThree: "Jetzt starten",
  meetJoinPlaceholder: "Meeting-Code oder alo-Link eingeben",
  meetJoinShort: "Beitreten",
  meetNew: "Neues Meeting",
  meetYourSpaceLead: "Ihr",
  meetYourSpaceAccent: "Meeting-Bereich",
  meetHeroNewTitle: "Treffen Sie sich mit einem Klick",
  meetHeroNewText:
    "Anrufe in hoher Qualität mit Bildschirmfreigabe, Chat, Reaktionen und einem Gerätecheck, bevor jemand Sie sieht oder hört.",
  meetSchedule: "Planen",
  meetJoinInputInvalid:
    "Geben Sie einen gültigen alo-Meeting-Link oder Meeting-Code ein.",
  meetUpcoming: "Anstehende Meetings",
  meetUpcomingHint: "Was laut Ihrem Kalender als Nächstes ansteht.",
  meetRecent: "Letzte Meetings",
  meetRecentHint:
    "Anrufe, an denen Sie teilnehmen konnten, als Verlauf des Arbeitsbereichs aufbewahrt.",
  meetEndedAt: (time: string) => `Beendet ${time}`,
  meetDuration: (minutes: number) => `${minutes} Min.`,
  meetCalendarUntitled: "Unbenannter Termin",
  meetSafetyTitle: "Der Zutritt bleibt unter Ihrer Kontrolle",
  meetSafetyBody:
    "Der Arbeitsbereich prüft den Zugriff, bevor er ein Medien-Token ausstellt. Ein Meeting-Code umgeht die Autorisierung nie.",
  meetTodaySchedule: "Heutige Termine",
  meetOpenAgenda: "Kalender öffnen",
  meetNoEventsToday: "Für heute ist nichts Weiteres geplant.",
  meetViewAgenda: "Vollständigen Kalender ansehen",
  meetQuickActions: "Schnellaktionen",
  meetLinkCopied: "Link kopiert",
  meetSomeone: "Jemand",
  meetHandsRaised: (names: string) => `Hand gehoben: ${names}`,
  meetNoEngine:
    "Meetings sind für diesen Arbeitsbereich noch nicht eingeschaltet. Das Meeting ist festgehalten, und alle Eingeladenen können es sehen — es gibt nur noch keinen Ort, es abzuhalten, bis ein Administrator den Meeting-Server konfiguriert.",

  // small shared words (first used by chat/meet)
  add: "Hinzufügen",
  save: "Speichern",
  deleteLabel: "Löschen",
  agentApprove: "Genehmigen",
  agentDiscard: "Verwerfen",

  // admin console
  adminTitle: "Admin",
  adminBackToalo: "Zurück zu alo",
  adminOpen: "Admin-Konsole",

  // admin — overview dashboard
  adminOverview: "Übersicht",
  adminOverviewIntro: "Ihre Organisation auf einen Blick.",
  overviewUsers: "Benutzer",
  overviewStorage: "Belegter Speicher",
  overviewDeliverability: "Zustellbarkeit",
  overviewDeliverOk: "Alle Prüfungen bestanden",
  overviewDeliverAttention: "Braucht Aufmerksamkeit",
  overviewAi: "KI",
  overviewOn: "Ein",
  overviewOff: "Aus",
  overviewManage: "Verwalten",

  // admin — domains (tenant's own; ADR 0012)
  adminDomains: "Domains",
  adminDomainsIntro:
    "Die Domains, über die diese Organisation E-Mails sendet und empfängt, und deren Verifizierung.",
  adminDomainsError: "Domains konnten nicht geladen werden.",
  adminDomainsEmpty:
    "Noch keine Domains. Fügen Sie eine hinzu, um sie zu verifizieren.",
  adminAddDomain: "Domain hinzufügen",
  dkimPublish:
    "Veröffentlichen Sie diesen DKIM-Eintrag, damit Ihre E-Mails signiert werden",
  dkimRotate: "DKIM-Schlüssel wechseln",
  dkimRotateConfirm: (domain: string) =>
    `DKIM-Schlüssel für ${domain} wechseln? Veröffentlichen Sie den neuen Eintrag; behalten Sie den alten, bis keine E-Mail ihn mehr verwendet.`,
  dkimRotated: (domain: string) =>
    `Neuer DKIM-Schlüssel für ${domain} — veröffentlichen Sie den aktualisierten Eintrag.`,

  // admin — audit log
  adminAudit: "Audit-Protokoll",
  adminAuditIntro: "Wer wann was geändert hat. Neueste zuerst.",
  adminAuditError: "Das Audit-Protokoll konnte nicht geladen werden.",
  adminAuditEmpty: "Noch keine administrativen Aktionen verzeichnet.",
  auditBy: (actor: string) => `von ${actor}`,
  auditUnknownActor: "System",
  auditUserCreate: "Benutzer angelegt",
  auditUserDelete: "Benutzer gelöscht",
  auditUserAdmin: "Admin-Rechte geändert",
  auditAliasAdd: "Alias hinzugefügt",
  auditAliasRemove: "Alias entfernt",
  auditGroupCreate: "Gruppe erstellt",
  auditGroupDelete: "Gruppe gelöscht",
  auditGroupAddress: "Verteileradresse geändert",
  auditDomainRegister: "Domain registriert",
  auditDomainVerify: "Domain verifiziert",
  auditDomainDelete: "Domain entfernt",
  auditTenantCreate: "Organisation angelegt",
  auditTenantStatus: "Status der Organisation geändert",
  auditTenantQuota: "Speicherkontingent geändert",

  // control plane (platform operator; ADR 0012). fr says "plan de
  // contrôle", nl "beheerplatform" — German follows the Dutch instinct
  // and names the thing by what it does.
  controlOpen: "Plattformverwaltung",
  controlTitle: "Plattformverwaltung",
  controlDeniedTitle: "Betreiberzugriff erforderlich",
  controlDeniedBody:
    "Die Plattformverwaltung ist den Betreibern der Plattform vorbehalten. Ihr Konto gehört nicht dazu — bitten Sie einen Betreiber, falls Sie Zugriff benötigen.",
  controlTenants: "Organisationen",
  controlTenantsIntro: "Jede Organisation auf dieser Installation.",
  controlTenantsError: "Organisationen konnten nicht geladen werden.",
  controlTenantsEmpty: "Noch keine Organisationen. Legen Sie die erste an.",
  controlDomains: "Domains",
  controlDomainsIntro:
    "Die Domains, über die jede Organisation E-Mails senden und empfangen darf, und deren Verifizierung.",
  controlDomainsError: "Domains konnten nicht geladen werden.",
  controlDomainsEmpty: "Noch keine Domains registriert.",
  tenantAdd: "Neue Organisation",
  tenantName: "Name der Organisation",
  tenantNameHint: "Acme GmbH",
  tenantAdminEmail: "E-Mail des ersten Admins",
  tenantAdminPassword: "Passwort des ersten Admins",
  tenantAdminPasswordHint: "mindestens 12 Zeichen",
  tenantCreate: "Organisation anlegen",
  tenantInvalid:
    "Erforderlich sind ein Name, eine gültige Admin-E-Mail-Adresse und ein Passwort mit mindestens 12 Zeichen.",
  tenantCreateError: "Die Organisation konnte nicht angelegt werden.",
  tenantActive: "Aktiv",
  tenantSuspended: "Gesperrt",
  tenantSuspend: "Sperren",
  tenantResume: "Entsperren",
  tenantDelete: "Organisation löschen",
  tenantDeleteConfirm: (name: string) =>
    `„${name}“ mit allen Daten endgültig löschen? Das lässt sich nicht rückgängig machen.`,
  tenantUsage: (n: number, size: string) => `${n} Benutzer · ${size}`,
  tenantQuota: "Kontingent",
  tenantQuotaPrompt: "Speicherkontingent in GB (leer lassen für unbegrenzt):",
  tenantQuotaUnlimited: "unbegrenzt",
  tenantQuotaOf: (size: string) => `von ${size}`,
  domainAdd: "Domain hinzufügen",
  domainTenant: "Zugehörige Organisation",
  domainName: "Domain",
  domainRegister: "Registrieren",
  domainInvalid:
    "Wählen Sie eine Organisation und geben Sie eine gültige Domain ein.",
  domainCreateError: "Die Domain konnte nicht registriert werden.",
  domainActionError: "Das hat nicht geklappt. Versuchen Sie es erneut.",
  domainVerified: "Verifiziert",
  domainUnverified: "Nicht verifiziert",
  domainVerify: "Verifizieren",
  domainDelete: "Domain entfernen",
  domainOwnedBy: (tenant: string) => `Gehört zu ${tenant}`,
  domainDeleteConfirm: (domain: string) =>
    `${domain} aus dieser Installation entfernen?`,
  domainVerifiedOk: (domain: string) => `${domain} ist verifiziert.`,
  domainVerifyPending: (domain: string) =>
    `Für ${domain} wurde noch kein passender DNS-TXT-Eintrag gefunden — veröffentlichen Sie ihn und versuchen Sie es erneut.`,
  domainPublishTitle: "Veröffentlichen Sie diesen DNS-Eintrag",
  domainPublishIntro: (domain: string) =>
    `Um zu belegen, dass ${domain} Ihnen gehört, veröffentlichen Sie diesen TXT-Eintrag und klicken Sie dann bei der Domain auf „Verifizieren“.`,
  domainRecordName: "Eintragsname",
  domainRecordType: "Typ",
  domainRecordValue: "Wert",
  domainPublishDone: "Fertig",

  adminDeniedTitle: "Admin-Zugriff erforderlich",
  adminDeniedBody:
    "Sie haben keinen Administratorzugriff auf diesen Arbeitsbereich. Bitten Sie einen Admin darum, falls Sie ihn benötigen.",
  adminSecurity: "Sicherheit & Vertrauen",
  adminSecurityIntro:
    "Wie Ihre Mail-Domain von außen aussieht. Diese Prüfungen fragen bei jedem Lauf live das DNS und die MTA-STS-Richtlinie ab.",
  securityFor: (domain: string) => `Prüfungen für ${domain}`,
  securityRecheck: "Prüfungen erneut ausführen",
  securityChecking: "Live-Prüfungen laufen…",
  securityError:
    "Die Prüfungen konnten nicht ausgeführt werden — bitte versuchen Sie es erneut.",
  securityPass: "Bestanden",
  securityWarn: "Achtung",
  securityFail: "Handlungsbedarf",
  adminGroups: "Gruppen & Verteiler",
  adminGroupsIntro:
    "Gruppen für gemeinsamen Zugriff und Verteilerlisten, die eingehende E-Mails an ihre Mitglieder verteilen.",
  adminNewGroup: "Neue Gruppe",
  adminGroupsError: "Gruppen konnten nicht geladen werden.",
  groupName: "Gruppenname",
  groupRename: "Umbenennen",
  groupCreate: "Gruppe erstellen",
  groupListBadge: "Verteiler",
  groupMembers: "Mitglieder",
  groupMemberCount: (n: number) => (n === 1 ? "1 Mitglied" : `${n} Mitglieder`),
  groupNoMembers: "Noch keine Mitglieder.",
  groupListAddress: "Verteileradresse",
  groupListAddressHint:
    "E-Mails an diese Adresse werden jedem Mitglied zugestellt. Leer lassen für eine reine Zugriffsgruppe.",
  groupAddressSave: "Adresse speichern",
  groupAddressClear: "Verteiler abschalten",
  groupAddMember: "Mitglied hinzufügen",
  groupDelete: "Gruppe löschen",
  groupDeleteConfirm: (name: string) =>
    `Die Gruppe „${name}“ löschen? Die Mitglieder behalten ihre Postfächer.`,
  groupCreateError:
    "Die Gruppe konnte nicht erstellt werden — der Name ist womöglich schon vergeben.",
  groupAddressError:
    "Die Adresse konnte nicht gesetzt werden — sie wird womöglich schon verwendet.",
  groupActionError: "Das hat nicht geklappt — bitte versuchen Sie es erneut.",
  groupClose: "Schließen",
  adminUsers: "Benutzer & Postfächer",
  adminUsersIntro: "Die Personen in Ihrer Organisation und ihre Postfächer.",
  adminAddUser: "Benutzer hinzufügen",
  adminUsersError: "Benutzer konnten nicht geladen werden.",
  userAdminBadge: "Admin",
  userManage: "Verwalten",
  userUsage: (n: number, size: string) =>
    `${n === 1 ? "1 Nachricht" : `${n} Nachrichten`} · ${size}`,
  userEmail: "E-Mail-Adresse",
  userPassword: "Passwort",
  userNewPassword: "Neues Passwort",
  userPasswordHint: "Mindestens 8 Zeichen.",
  userCreate: "Benutzer anlegen",
  userInvalid:
    "Geben Sie eine gültige E-Mail-Adresse und ein Passwort mit mindestens 8 Zeichen ein.",
  userCreateError:
    "Der Benutzer konnte nicht angelegt werden — die E-Mail-Adresse wird womöglich schon verwendet.",
  userReset: "Passwort zurücksetzen",
  userResetDone: "Passwort zurückgesetzt.",
  userAdminRole: "Admin der Organisation",
  userAdminRoleFor: (email: string) => `Admin-Zugriff für ${email}`,
  userAdminHint: "Admins können Benutzer, Aliasse und Einstellungen verwalten.",
  userRoles: "Rollen",
  userInvite: "Einladung erstellen",
  userInviteReady: "Einrichtungslink",
  userInviteCopy: "Kopieren",
  userInviteCopied: "Kopiert",
  userInviteHint:
    "Senden Sie diesen Link an Ihre Kollegin oder Ihren Kollegen. Er funktioniert genau einmal, läuft nach sieben Tagen ab, und die Person wählt Passwort und Wiederherstellungsadresse selbst — Sie erfahren beides nie. Dieser Link wird nur einmal angezeigt.",
  inviteTitle: "Richten Sie Ihr Konto ein",
  inviteUnavailable: "Diese Einladung funktioniert nicht mehr",
  inviteAskAdmin:
    "Bitten Sie den Administrator Ihres Arbeitsbereichs um eine neue.",
  inviteLoadFailed:
    "Diese Einladung ist abgelaufen oder wurde bereits verwendet.",
  inviteFailed: "Das konnte nicht gespeichert werden. Versuchen Sie es erneut.",
  invitePassword: "Wählen Sie ein Passwort",
  invitePasswordHint: "Mindestens 8 Zeichen. Nur Sie werden es kennen.",
  inviteRecovery: "Wiederherstellungsadresse",
  inviteRecoveryPlaceholder: "sie@woanders.de",
  inviteRecoveryHint:
    "Eine Adresse, die Sie woanders abrufen können — nicht diese neue. Falls Sie Ihr Passwort einmal vergessen, ist sie der einzige Weg zurück, ohne einen Administrator zu fragen.",
  inviteSubmit: "Konto einrichten",
  inviteWorking: "Wird eingerichtet…",
  inviteDoneTitle: "Alles bereit",
  inviteGoToSignIn: "Zur Anmeldung",
  inviteFor: (email: string): string => `Für ${email}`,
  inviteDoneBody: (email: string): string =>
    `Sie können sich jetzt als ${email} anmelden, mit dem Passwort, das Sie gerade gewählt haben.`,
  userApps: "Apps",
  userAppsHint:
    "Nur die angehakten Apps erscheinen in der Navigation dieser Person, und der Server verweigert alle übrigen — das versteckt nicht nur, es schließt ab. E-Mail und Start lassen sich nicht abschalten. Ein Häkchen gewährt nicht alles darin: Finanzen verlangt weiterhin die Buchhaltungsrolle, und ein Space weiterhin die Mitgliedschaft.",
  userAppsSelfHint:
    "Dies ist Ihr eigenes Konto. Ein Admin wird nie ausgesperrt, daher ändern diese Schalter nichts daran, was Sie öffnen können — sie bleiben erhalten für den Fall, dass dieses Konto einmal kein Admin mehr ist.",
  accessModuleOff: "Diese App ist für Ihr Konto abgeschaltet.",
  accessModuleOffHint:
    "Ein Administrator des Arbeitsbereichs kann sie wieder einschalten.",
  accessBackHome: "Zurück zum Start",
  userAccountantRole: "Buchhaltung",
  userAccountantHint:
    "Liest die Bücher — Berichte, Spesenfreigaben und den Periodenabschluss — und kann Rechnungen und Deals öffnen, ohne sie zu ändern. Keine Admin-Konsole und kein Zugriff auf fremde E-Mails oder Dateien.",
  userAccountantBadge: "Buchhaltung",
  userAliases: "Aliasse",
  userAliasesHint: "Zusätzliche Adressen, die in dieses Postfach zustellen.",
  userAliasPlaceholder: "alias@namel3ss.com",
  userAliasAdd: "Alias hinzufügen",
  userDelete: "Benutzer löschen",
  userDeleteConfirm: (email: string) =>
    `${email} mit sämtlichen E-Mails löschen? Das lässt sich nicht rückgängig machen.`,
  userActionError: "Das hat nicht geklappt — bitte versuchen Sie es erneut.",
  userClose: "Schließen",

  // admin — AI providers. Provider and model names stay as they are;
  // "alo AI" is a product name like Space or Base.
  adminAiProviders: "KI-Anbieter",
  adminProviderEnabledFor: (name: string) => `${name} aktiviert`,
  adminAiIntro:
    "Wählen Sie, welche Modelle alo antreiben — selbst gehostet oder mit Ihren eigenen API-Schlüsseln.",
  adminAddProvider: "Anbieter hinzufügen",
  adminManage: "Verwalten",
  adminDefaultBadge: "Standard",
  adminMakeDefault: "Als Standard festlegen",
  adminProvidersError: "Anbieter konnten nicht geladen werden.",
  adminAiSelfHosted: "Selbst gehostet (empfohlen)",
  adminAiSelfHostedHint:
    "Läuft auf Ihrer eigenen Infrastruktur — keine Daten verlassen Ihre Server.",
  adminAiOwnKeys: "Ihre eigenen API-Schlüssel",
  adminAiOwnKeysHint:
    "Verbinden Sie einen externen Anbieter mit Ihrem Schlüssel. Anfragen verlassen Ihren Server zu diesem Anbieter.",
  adminAiFootnote:
    "Selbst gehostete Anbieter behalten alle Daten auf Ihrer Infrastruktur. Externe API-Schlüssel senden Anfragen und Inhalte an den jeweiligen Anbieter — entscheiden Sie nach Ihrer Datenrichtlinie.",
  providerConnected: "Verbunden",
  providerKeyAdded: "Schlüssel hinterlegt",
  providerReady: "Bereit",
  providerNotConfigured: "Nicht konfiguriert",
  kindOllama: "Ollama",
  kindalo: "alo AI",
  kindMistral: "Mistral (EU)",
  kindOpenai: "OpenAI",
  kindAnthropic: "Anthropic",
  kindCustom: "Eigener Endpunkt",
  builtInTag: "Integriert",
  ollamaDesc:
    "Lokale Modelle auf Ihrem Server — Llama 3, Mistral und mehr. Vollständig privat.",
  aloDesc:
    "Integriertes, in der EU gehostetes Modell, auf alo abgestimmt — richten Sie es auf Ihren alo-AI-Endpunkt.",
  mistralDesc:
    "Europäische Modelle, gehostet in der EU. Hinterlegen Sie Ihren Mistral-Schlüssel, um sie zu aktivieren. Empfohlen für Datensouveränität.",
  openaiDesc:
    "GPT-4o, GPT-4o mini. Hinterlegen Sie Ihren OpenAI-Schlüssel, um sie zu aktivieren.",
  anthropicDesc:
    "Claude-Modelle. Hinterlegen Sie Ihren Anthropic-API-Schlüssel, um sie zu aktivieren.",
  customDesc:
    "Jede OpenAI-kompatible API — selbst gehostetes vLLM, Together, Groq, OpenRouter …",
  connectTitle: (name: string) => `${name} verbinden`,
  configureTitle: (name: string) => `${name} konfigurieren`,
  providerBaseUrl: "API-Endpunkt",
  providerModel: "Modell",
  providerModels: "Aktivierte Modelle",
  providerAddModel: "Hinzufügen",
  providerModelPlaceholder: "Modellname",
  providerRemoveModel: (name: string) => `${name} entfernen`,
  providerApiKey: "API-Schlüssel",
  providerShowKey: "Schlüssel anzeigen",
  providerHideKey: "Schlüssel verbergen",
  providerApiKeyKept:
    "Gespeichert — leer lassen, um den aktuellen Schlüssel zu behalten",
  providerApiKeyOptional: "Für ein lokales Ollama nicht nötig",
  providerTest: "Verbindung testen",
  providerTestAgain: "Erneut testen",
  providerTesting: "Wird getestet…",
  providerTestOk: (n: number) =>
    n === 1
      ? "Verbindung geprüft — 1 Modell erreichbar"
      : `Verbindung geprüft — ${n} Modelle erreichbar`,
  providerTestFail: "Der Endpunkt ist nicht erreichbar.",
  providerCancel: "Abbrechen",
  providerSave: "Speichern & aktivieren",
  providerSaveError: "Der Anbieter konnte nicht gespeichert werden.",
  providerRequired: "Ein Endpunkt und ein Modell sind erforderlich.",

  // compose recipients + archive (mail strays tranche 1's prefixes missed)
  removeRecipient: (name: string) => `${name} entfernen`,
  recipientCount: (n: number) => `${n} Empfänger`,
  archiveUnavailable:
    "Es gibt keinen Archivordner, in den sich dies verschieben ließe.",

  // Audit trail — a record's own history (B2.13). Verbs, not sentences,
  // and past tense, as in every language. Zurückgewiesen (a timesheet
  // sent back) and Abgelehnt (a quote the customer declined) stay two
  // words because they are two different acts.
  auditHistoryTitle: "Verlauf",
  auditHistoryEmpty: "Mit diesem Eintrag ist noch nichts passiert.",
  auditLoadFailed: "Der Verlauf konnte nicht geladen werden.",
  auditActionCreate: "Angelegt",
  auditActionUpdate: "Bearbeitet",
  auditActionDelete: "Gelöscht",
  auditActionArchive: "Archiviert",
  auditActionIssue: "Ausgestellt",
  auditActionVoid: "Storniert",
  auditActionCreditNote: "Gutschrift erstellt",
  auditActionSend: "E-Mail entworfen",
  auditActionReminder: "Zahlungserinnerung entworfen",
  auditActionPaymentCreate: "Zahlung erfasst",
  auditActionPaymentDelete: "Zahlung entfernt",
  auditActionImport: "Importiert",
  auditActionSepaXml: "In eine Zahlungsdatei aufgenommen",
  auditActionApprove: "Genehmigt",
  auditActionReject: "Zurückgewiesen",
  auditActionAccept: "Angenommen",
  auditActionDecline: "Abgelehnt",
  auditActionExpire: "Als abgelaufen markiert",
  auditActionRun: "Ausgeführt",
  auditActionPause: "Pausiert",
  auditActionResume: "Fortgesetzt",
  auditActionRatesUpdate: "Wechselkurs festgelegt",
  auditActionRatesImport: "Wechselkurse importiert",
  auditActionStageMove: "In eine andere Spalte verschoben",
  auditActionStageCreate: "Spalte hinzugefügt",
  auditActionMove: "Verschoben",
  auditActionQuoteRaised: "Angebot erstellt",
  auditActionInvoiceRaised: "Rechnung erstellt",
  auditActionActivityCreate: "Notiz hinzugefügt",
  auditActionNextStepCreate: "Nächster Schritt hinzugefügt",
  auditActionThreadCreate: "Unterhaltung verknüpft",
  auditActionThreadDelete: "Unterhaltung getrennt",
  auditActionLeadCreate: "Leads importiert",

  // The agent's proposal frame, and the billing/CRM tool cards (tranche 5).
  // Every note keeps the English promise word for word: a draft, never an
  // issued document, and nothing is ever sent by approving.
  agentProposedAction:
    "alo möchte Folgendes tun — genehmigen Sie es, um fortzufahren.",
  agentDone: "Erledigt.",
  agentFailed: "Diese Aktion konnte nicht abgeschlossen werden.",
  agentActInvoiceDraft: "Rechnung entwerfen",
  agentActQuoteToInvoice: "Angebot annehmen",
  agentActPaymentReminder: "Zahlungserinnerung",
  agentFieldCustomer: "Kunde",
  agentFieldLines: "Positionen",
  agentFieldQuote: "Angebot",
  agentFieldInvoice: "Rechnung",
  agentLineCount: (n: number): string =>
    n === 1 ? "1 Position" : `${n} Positionen`,
  agentInvoiceDraftNote:
    "Erstellt einen Entwurf — nichts wird ausgestellt, nummeriert oder gesendet.",
  agentQuoteToInvoiceNote:
    "Schließt das Angebot als angenommen ab und erstellt einen Rechnungsentwurf.",
  agentReminderNote:
    "Schreibt eine Erinnerung in Entwürfe — nichts wird gesendet.",
  agentActCreateDeal: "Neuer Deal",
  agentActMoveDeal: "Deal verschieben",
  agentActFollowup: "Nachfass-E-Mail",
  agentFieldDeal: "Deal",
  agentFieldCompany: "Unternehmen",
  agentFieldValue: "Wert",
  agentFieldStage: "Phase",
  agentFieldLostReason: "Verloren, weil",
  agentDealFromEmailNote: "Verknüpft diese Unterhaltung mit dem neuen Deal.",
  agentFollowupNote: "Schreibt die E-Mail in Entwürfe — nichts wird gesendet.",

  // alo Billing (tranche 5). The module speaks about documents, never rows,
  // and a German invoice speaks its trade's own words: ausstellen spends the
  // number, stornieren voids, a Gutschrift corrects, das Zahlungsziel is the
  // terms. MwSt. on amounts, USt-IdNr. for the identifier — the split German
  // paperwork itself makes. (billingWorkspacePurpose shipped with the app
  // launcher in tranche 1 and lives beside the module labels above.)
  billingCustomers: "Kunden",
  billingProducts: "Preisliste",
  billingSearchCustomers: "Kunden durchsuchen…",
  billingSearchProducts: "Preisliste durchsuchen…",
  billingShowArchived: "Archivierte anzeigen",
  billingArchived: "Archiviert",
  billingArchive: "Archivieren",
  billingRestore: "Wiederherstellen",
  billingNewCustomer: "Neuer Kunde",
  billingNewProduct: "Neuer Artikel",
  billingEditCustomer: "Kunde bearbeiten",
  billingEditProduct: "Artikel bearbeiten",
  billingCustomerSubtitle: "Auf wen Ihre Rechnungen ausgestellt werden.",
  billingProductSubtitle:
    "Ein Artikel, den Sie auswählen können, wenn Sie ein Dokument erstellen.",
  billingArchiveCustomerConfirm: (name: string) =>
    `${name} archivieren? Der Kunde verschwindet aus der Auswahl; jedes bereits erstellte Dokument nennt ihn weiterhin.`,
  billingArchiveProductConfirm: (name: string) =>
    `${name} archivieren? Der Artikel verschwindet aus der Auswahl; bereits erstellte Dokumente behalten den Preis, zu dem sie erstellt wurden.`,
  billingCreate: "Erstellen",
  billingSave: "Speichern",
  billingCancel: "Abbrechen",
  billingLoadFailed:
    "Diese Liste konnte nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  billingLoading: "Rechnungsdaten werden geladen…",
  billingPaginationLabel: "Seiten der Abrechnungsliste",
  billingPaginationPrevious: "Vorherige Seite",
  billingPaginationNext: "Nächste Seite",
  billingPaginationRange: (first: number, last: number, total: number) =>
    `${first}–${last} von ${total}`,
  billingPaginationPage: (page: number, total: number) =>
    `Seite ${page} von ${total}`,
  billingSaveFailed:
    "Speichern nicht möglich. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  billingNoMatches: "Nichts entspricht dieser Suche.",
  billingNoCustomersTitle: "Noch keine Kunden",
  billingNoCustomersBody:
    "Ein Kunde trägt die Adresse, die USt-IdNr. und das Zahlungsziel, mit denen jede Rechnung für ihn beginnt.",
  billingGetStarted: "In 3 einfachen Schritten loslegen",
  billingStepCustomerTitle: "Legen Sie Ihren ersten Kunden an",
  billingStepCustomerBody:
    "Erstellen Sie ein Kundenprofil mit den Rechnungsangaben.",
  billingStepInvoiceTitle: "Erstellen Sie Ihre erste Rechnung",
  billingStepInvoiceBody:
    "Positionen hinzufügen, Zahlungsziel festlegen, ausstellen.",
  billingStepPaidTitle: "Schneller bezahlt werden",
  billingStepPaidBody:
    "Zahlungen erfassen und die Liquidität im Blick behalten.",
  billingNoProductsTitle: "Ihre Preisliste ist leer",
  billingNoProductsBody:
    "Erfassen Sie einmal, was Sie verkaufen, und wählen Sie es dann aus, wenn Sie ein Angebot oder eine Rechnung erstellen.",
  billingColName: "Name",
  billingColLocation: "Ort",
  billingColVatId: "USt-IdNr.",
  billingColEmail: "E-Mail",
  billingColTerms: "Zahlungsziel",
  billingColCurrency: "Währung",
  billingColUnit: "Einheit",
  billingColUnitPrice: "Einzelpreis",
  billingColVatRate: "MwSt.-Satz",
  billingColActions: "Aktionen",
  billingTermsDays: (days: number) => (days === 1 ? "1 Tag" : `${days} Tage`),
  billingFieldName: "Name",
  billingFieldEmail: "Rechnungs-E-Mail",
  billingFieldAddress: "Adresse",
  billingFieldAddress2: "Adresse, zweite Zeile",
  billingFieldPostalCode: "Postleitzahl",
  billingFieldCity: "Ort",
  billingFieldCountry: "Land",
  billingFieldVatId: "USt-IdNr.",
  billingFieldTerms: "Zahlungsziel (Tage)",
  billingFieldCurrency: "Währung",
  billingFieldUnit: "Einheit",
  billingFieldUnitPrice: "Einzelpreis",
  billingFieldVatRate: "MwSt.-Satz (%)",
  billingEmailPlaceholder: "rechnung@beispiel.de",
  billingAddressPlaceholder: "Straße und Hausnummer",
  billingCountryPlaceholder: "BE",
  billingCountryHint: "Zweibuchstabiger Ländercode.",
  billingCurrencyPlaceholder: "EUR",
  billingVatIdPlaceholder: "BE0123456789",
  billingVatIdHint: "Für Privatkunden leer lassen.",
  billingTermsPlaceholder: "30",
  billingTermsHint: "Tage von der Ausstellung bis zur Fälligkeit.",
  billingUnitPlaceholder: "Stunde",
  billingUnitHint:
    "Wie eine Einheit davon heißt. Für einen Pauschalartikel leer lassen.",
  billingAmountPlaceholder: "0,00",
  billingPriceHint: "Ohne MwSt.",
  billingRatePlaceholder: "21",
  billingRateHint: "0 für einen steuerbefreiten Artikel.",
  billingNotAnAmount: "Geben Sie einen Betrag wie 1250,00 ein.",
  billingNotARate: "Geben Sie einen Satz wie 21 ein.",
  billingInvoices: "Rechnungen",
  billingNewInvoice: "Neue Rechnung",
  billingSearchInvoices: "Nach Nummer, Kunde oder Referenz suchen…",
  billingFilterStatus: "Anzeigen",
  billingFilterAll: "Alle Dokumente",
  billingStatusDraft: "Entwurf",
  billingStatusIssued: "Ausgestellt",
  billingStatusPaid: "Bezahlt",
  billingStatusVoid: "Storniert",
  billingStatusOverdue: "Überfällig",
  billingCreditNote: "Gutschrift",
  billingCreditNotes: "Gutschriften",
  billingNoInvoicesTitle: "Noch keine Rechnungen",
  billingNoInvoicesBody:
    "Erstellen Sie einen Entwurf für einen Kunden, fügen Sie hinzu, was Sie berechnen, und stellen Sie ihn aus, wenn er stimmt.",
  billingColNumber: "Nummer",
  billingColCustomer: "Kunde",
  billingColIssueDate: "Ausstellungsdatum",
  billingColDueDate: "Fälligkeitsdatum",
  billingColStatus: "Status",
  billingColTotal: "Gesamt",
  billingColDescription: "Beschreibung",
  billingColQty: "Menge",
  billingColNet: "Netto",
  billingNotNumbered: "—",
  billingNoDate: "—",
  billingUnknownCustomer: "Unbekannter Kunde",
  billingDraftInvoice: "Rechnungsentwurf",
  billingBackToInvoices: "Alle Rechnungen",
  billingBackToProject: (name: string) => `Zurück zu ${name}`,
  billingInvoiceGone: "Dieses Dokument existiert nicht mehr.",
  billingFieldCustomer: "Kunde",
  billingChooseCustomer: "Kunde wählen…",
  billingCustomerFixedHint:
    "Währung und Zahlungsziel des Kunden werden auf das Dokument übernommen.",
  billingFieldReference: "Kundenreferenz (optional)",
  billingReferencePlaceholder: "Zum Beispiel PO-1234",
  billingReferenceHint:
    "Geben Sie eine Bestell- oder Angebotsanfragenummer des Kunden ein. Alo vergibt beim Finalisieren automatisch eine eigene eindeutige Nummer für dieses Dokument.",
  billingFieldNote: "Anmerkung",
  billingNotePlaceholder: "Alles, was der Kunde auf dem Dokument lesen soll.",
  billingNoteHint: "Wird unter den Positionen gedruckt.",
  billingFieldIssueDate: "Ausstellungsdatum",
  billingFieldDueDate: "Fälligkeitsdatum",
  billingCreateDraft: "Entwurf erstellen",
  billingCreateDraftHint:
    "Zuerst wird der Entwurf erstellt; dann fügen Sie hinzu, was Sie berechnen.",
  billingLines: "Positionen",
  billingAddLine: "Position hinzufügen",
  billingRemoveLine: "Diese Position entfernen",
  billingNoLines: "Noch nichts auf diesem Dokument.",
  billingPickProduct: "Aus der Preisliste…",
  billingDescriptionPlaceholder: "Was Sie berechnen",
  billingQtyPlaceholder: "1",
  billingLineNeedsDescription:
    "Eine Position braucht eine Beschreibung, bevor der Entwurf gespeichert werden kann.",
  billingNotAQuantity: "Geben Sie eine Menge wie 1,5 ein.",
  billingTotalsNet: "Netto",
  billingTotalsGross: "Gesamt",
  billingVatAtRate: (rate: string) => `MwSt. ${rate}`,
  billingTotalsStale:
    "Dies sind die zuletzt vom Server gesendeten Beträge; sie aktualisieren sich, sobald der Entwurf gespeichert ist.",
  billingSaving: "Wird gespeichert…",
  billingSaved: "Gespeichert",
  billingUnsaved: "Noch nicht gespeichert",
  billingSaveNotDone: "Speichern nicht möglich",
  billingSaveNow: "Erneut versuchen",
  billingDeleteDraft: "Entwurf löschen",
  billingDeleteDraftConfirm:
    "Diesen Entwurf löschen? Er trägt keine Nummer, es bleibt also nichts zurück — und nichts lässt sich wiederherstellen.",
  billingFrozenNotice:
    "Dieses Dokument trägt eine Nummer und kann nicht mehr geändert werden. Korrigieren Sie es mit einer Gutschrift.",
  billingActionFailed:
    "Das hat nicht geklappt. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  billingActionsWaitForSave:
    "Diese warten, bis Ihre letzte Änderung gespeichert ist.",
  billingIssue: "Ausstellen und E-Mail vorbereiten",
  billingIssueTitle: "Ausstellen und Kunden-E-Mail vorbereiten?",
  billingIssueConfirm:
    "Dadurch erhält die Rechnung die nächste Nummer, wird datiert und gesperrt. Anschließend öffnet sich eine vollständige Kunden-E-Mail mit angehängter PDF, die Sie vor dem Versand in Mail prüfen.",
  billingPrepareInvoiceEmail: "Kunden-E-Mail vorbereiten",
  billingPrepareInvoiceEmailTitle: "Diese Rechnung für den Kunden vorbereiten?",
  billingPrepareInvoiceEmailConfirm:
    "In Mail wird eine vollständige E-Mail an den Kunden mit dieser Rechnung im Anhang geöffnet. Nichts wird versendet, bevor Sie auf Senden drücken.",
  billingVoid: "Stornieren",
  billingVoidTitle: "Diese Rechnung stornieren?",
  billingVoidConfirm:
    "Eine stornierte Rechnung behält ihre Nummer und bleibt lesbar, ist aber nichts mehr wert. Stornieren Sie eine, die niemand gesehen hat; hält der Kunde das Dokument bereits, erstellen Sie stattdessen eine Gutschrift.",
  billingVoidNotice:
    "Diese Rechnung wurde storniert. Sie behält ihre Nummer und ist nichts mehr wert.",
  billingCreditNoteAction: "Gutschrift",
  billingCreditNoteTitle: "Gutschrift erstellen?",
  billingCreditNoteConfirm:
    "Dies erstellt einen Gutschriftsentwurf, der jede Position dieser Rechnung spiegelt. Kürzen Sie ihn für eine Teilgutschrift und stellen Sie ihn dann wie jedes andere Dokument aus.",
  billingCreditsInvoice: "Die Rechnung, die hiermit gutgeschrieben wird",
  billingFromQuote: "Das Angebot, aus dem dies entstand",
  billingPayments: "Zahlungen",
  billingRecordPayment: "Zahlung erfassen",
  billingRecordPaymentHint:
    "Geld, das eingegangen ist. Es wird nirgendwohin überwiesen — hier wird nur festgehalten, was Ihre Bank bereits zeigt.",
  billingRemovePayment: "Entfernen",
  billingNoPayments: "Auf diese Rechnung ist noch nichts eingegangen.",
  billingPaidToDate: "Eingegangen",
  billingOutstanding: "Noch offen",
  billingOverpaidNote:
    "Es ist mehr eingegangen, als diese Rechnung wert ist. Die Differenz können Sie erstatten oder mit der nächsten verrechnen.",
  billingPaymentUnpaid: "Unbezahlt",
  billingPaymentPartiallyPaid: "Teilweise bezahlt",
  billingPaymentPaid: "Beglichen",
  billingColPaidOn: "Eingegangen am",
  billingColMethod: "Wie",
  billingColPaymentReference: "Bankreferenz",
  billingColAmount: "Betrag",
  billingFieldAmount: (currency: string) => `Betrag (${currency})`,
  billingFieldAmountHint:
    "Was tatsächlich eingegangen ist — das kann weniger als die Rechnung sein.",
  billingFieldPaidOn: "Eingegangen am",
  billingFieldPaidOnHint:
    "Der Tag, den Ihre Bank zeigt. Leer lassen für heute.",
  billingFieldMethod: "Wie es eingegangen ist",
  billingFieldMethodHint:
    "Freitext — wie auch immer Ihre Buchhaltung es nennt.",
  billingMethodPlaceholder: "Überweisung",
  billingFieldPaymentReference: "Bankreferenz",
  billingFieldPaymentRefHint:
    "Die Referenz auf der Umsatzzeile, damit sie sich später zuordnen lässt.",
  billingFilterOverdue: "Überfällig",
  billingColOutstanding: "Noch offen",
  billingReports: "MwSt.-Bericht",
  billingReportFrom: "Von",
  billingReportTo: "Bis",
  billingReportShow: "Anzeigen",
  billingReportThisQuarter: "Dieses Quartal",
  billingReportLastQuarter: "Letztes Quartal",
  billingReportDownloadCsv: "CSV herunterladen",
  billingReportDownloadFailed:
    "Die Datei konnte nicht erstellt werden. Versuchen Sie es erneut.",
  billingReportBasis: (from: string, to: string) =>
    `Ausgestellte und bezahlte Dokumente vom ${from} bis ${to}. Gutschriften werden abgezogen; Entwürfe und stornierte Dokumente zählen nicht.`,
  billingReportColVat: "MwSt.",
  billingReportTotal: "Gesamt",
  billingReportGross: "Inklusive MwSt.",
  billingReportOverview: "Übersicht der Meldung",
  billingReportTaxableNet: "Steuerpflichtiger Nettobetrag",
  billingReportVatDue: "Fällige MwSt.",
  billingReportGrossBilled: "Brutto fakturiert",
  billingReportDocuments: "Dokumente",
  billingReportCurrencyDetail: "Details nach Währung",
  billingReportCaption: (currency: string) => `MwSt.-Übersicht in ${currency}`,
  billingReportCounts: (invoices: number, creditNotes: number) =>
    `Aus ${invoices === 1 ? "1 Rechnung" : `${invoices} Rechnungen`} und ${creditNotes === 1 ? "1 Gutschrift" : `${creditNotes} Gutschriften`}.`,
  billingReportEmptyTitle: "In diesem Zeitraum wurde nichts ausgestellt",
  billingReportEmptyBody:
    "Ein Dokument zählt ab dem Tag seiner Ausstellung. Wählen Sie einen anderen Zeitraum, oder stellen Sie die Entwürfe aus, die in diesen gehören.",
  billingQuotes: "Angebote",
  billingQuotation: "Angebot",
  billingPreparedFor: "Exklusiv für diesen Kunden erstellt",
  billingIncludingVat: "Inklusive MwSt.",
  billingQuoteTemplate: "Angebotsvorlage",
  billingQuoteStartFrom: "Mit einer Vorlage beginnen",
  billingQuoteTemplateHint:
    "Nutzen Sie Ihre Preisliste für einen brauchbaren Ausgangspunkt.",
  billingQuoteTemplateBlank: "Leeres Angebot",
  billingQuoteTemplateBlankDescription:
    "Beginnen Sie mit einer leeren Preistabelle.",
  billingQuoteTemplateServices: "Professionelle Leistungen",
  billingQuoteTemplateServicesDescription:
    "Ein fokussiertes Angebot mit zwei Kernleistungen.",
  billingQuoteTemplateProject: "Projektumsetzung",
  billingQuoteTemplateProjectDescription:
    "Ein größerer Umfang mit drei Lieferpositionen.",
  billingQuoteTemplateRetainer: "Laufende Zusammenarbeit",
  billingQuoteTemplateRetainerDescription:
    "Beginnen Sie mit einer wiederkehrenden monatlichen Leistung.",
  quoteStudioTemplateServicesHeading: "Ausgewählte Leistungen für Sie",
  quoteStudioTemplateServicesIntroduction:
    "Eine klare Übersicht über die Leistungen, Ergebnisse und Investition, die wir für Ihr Unternehmen vorschlagen.",
  quoteStudioTemplateServicesTable: "Leistungen und Honorare",
  quoteStudioTemplateProjectHeading: "Projektangebot",
  quoteStudioTemplateProjectIntroduction:
    "Dieses Angebot fasst Projektumfang, Vorgehen und kaufmännische Bedingungen übersichtlich zusammen.",
  quoteStudioTemplateProjectDiscovery: "Analyse und Abstimmung",
  quoteStudioTemplateProjectDelivery: "Umsetzung und Prüfung",
  quoteStudioTemplateProjectHandover: "Einführung und Übergabe",
  quoteStudioTemplateProjectTable: "Projektinvestition",
  quoteStudioTemplateRetainerHeading: "Monatliche Partnerschaft",
  quoteStudioTemplateRetainerIntroduction:
    "Laufende Unterstützung mit planbarer monatlicher Investition und klarer Zusammenarbeit.",
  quoteStudioTemplateRetainerTable: "Monatliche Leistungen",
  quoteStudioTemplateRetainerReporting: "Regelmäßige Fortschrittsberichte",
  quoteStudioTemplateRetainerSupport: "Bevorzugte Unterstützung und Planung",
  billingQuoteIncludedItems: (count: number) =>
    count === 1 ? "1 Artikel" : `${count} Artikel`,
  billingQuoteIncludedTitle: "Artikel zum Hinzufügen bereit",
  billingQuoteIncludedHelp:
    "Prüfen Sie, was diese Vorlage hinzufügt. Mengen, Preise und Beschreibungen können Sie im Editor anpassen.",
  billingQuoteRemoveIncludedItem: (name: string) => `${name} entfernen`,
  billingQuoteAddFromPriceList: "Artikel hinzufügen",
  billingQuoteSearchPriceList: "Preisliste durchsuchen",
  billingQuoteAllItemsIncluded:
    "Jeder aktive Preislisten-Artikel ist bereits enthalten.",
  billingQuoteNoMatchingItems:
    "Kein Preislisten-Artikel entspricht dieser Suche.",
  billingQuotePerItem: "je",
  billingQuoteContinueToEditor: "Weiter zum Editor",
  billingNewQuote: "Neues Angebot",
  billingSearchQuotes: "Nach Nummer, Kunde oder Referenz suchen…",
  billingNoQuotesTitle: "Noch keine Angebote",
  billingNoQuotesBody:
    "Bieten Sie einem Kunden einen Preis an. Nimmt er an, wird das Angebot zu einem Rechnungsentwurf mit denselben Positionen.",
  billingQuoteStatusSent: "Finalisiert",
  billingQuoteStatusAccepted: "Angenommen",
  billingQuoteStatusDeclined: "Abgelehnt",
  billingQuoteStatusExpired: "Abgelaufen",
  billingQuoteLapsed: "Datum überschritten",
  billingColSentDate: "Finalisiert am",
  billingColValidUntil: "Gültig bis",
  billingColCreated: "Erstellt",
  billingColLastEdited: "Zuletzt bearbeitet",
  billingDraftQuote: "Angebotsentwurf",
  billingBackToQuotes: "Alle Angebote",
  billingQuoteGone: "Dieses Angebot existiert nicht mehr.",
  billingQuoteCustomerHint:
    "Die Währung des Kunden wird auf das Angebot übernommen.",
  billingCreateQuoteHint:
    "Zuerst wird der Entwurf erstellt; dann fügen Sie hinzu, was Sie anbieten.",
  billingFieldSentDate: "Finalisiert am",
  billingFieldValidUntil: "Gültig bis",
  billingValidForDays: (days: number) =>
    days === 1
      ? "Gültig für 1 Tag ab Finalisierung."
      : `Gültig für ${days} Tage ab Finalisierung.`,
  billingDeleteQuoteDraft: "Entwurf löschen",
  billingDeleteQuoteDraftConfirm:
    "Diesen Entwurf löschen? Er trägt keine Nummer und wurde niemandem unterbreitet — und nichts lässt sich wiederherstellen.",
  billingQuoteSentNotice: "In alo finalisiert und bereit für den Kunden.",
  billingQuoteClosedNotice:
    "Dieses Angebot ist abgeschlossen und kann nicht mehr geändert werden.",
  billingSendQuote: "Finalisieren und E-Mail vorbereiten",
  billingSendQuoteTitle: "Finalisieren und Kunden-E-Mail vorbereiten?",
  billingSendQuoteConfirm:
    "Dies vergibt die nächste Angebotsnummer, hält das Datum fest und sperrt die Preise. Anschließend öffnet sich eine vollständige Kunden-E-Mail mit angehängter PDF, die Sie vor dem Versand in Mail prüfen.",
  billingPrepareQuoteEmail: "Kunden-E-Mail vorbereiten",
  billingPrepareQuoteEmailTitle: "Dieses Angebot für den Kunden vorbereiten?",
  billingPrepareQuoteEmailConfirm:
    "In Mail wird eine vollständige E-Mail an den Kunden mit diesem Angebot im Anhang geöffnet. Nichts wird versendet, bevor Sie auf Senden drücken.",
  billingAcceptQuote: "Angebot annehmen",
  billingAcceptQuoteTitle: "Der Kunde hat angenommen?",
  billingAcceptQuoteConfirm:
    "Dies schließt das Angebot ab und erstellt einen Rechnungsentwurf mit denselben Positionen zu denselben Preisen. Noch wird nichts ausgestellt — Sie landen auf dem Entwurf.",
  billingDeclineQuote: "Angebot ablehnen",
  billingDeclineQuoteTitle: "Der Kunde hat abgelehnt?",
  billingDeclineQuoteConfirm:
    "Das Angebot schließt endgültig und bleibt lesbar. Ein Sinneswandel ist ein neues Angebot, kein wiedereröffnetes.",
  billingExpireQuote: "Als abgelaufen markieren",
  billingExpireQuoteTitle: "Dieses Angebot nicht weiter verfolgen?",
  billingExpireQuoteConfirm:
    "Das Angebot schließt als abgelaufen, mit heute als dem Tag, an dem Sie es aufgegeben haben. Es kann danach nicht mehr beantwortet werden.",
  billingQuoteInvoice: "Die Rechnung, die daraus wurde",
  billingPrint: "Drucken",
  billingPrintUnsaved:
    "Gedruckt wird das gespeicherte Dokument, daher wartet dies auf Ihre letzte Änderung.",
  billingPrintFailed:
    "Das Dokument konnte nicht zum Drucken vorbereitet werden. Versuchen Sie es erneut.",
  billingSettings: "Ihre Angaben",
  billingSettingsIntro:
    "Von wem Ihre Rechnungen, Gutschriften und Angebote stammen: der Name und die Nummern oben, und das Konto, auf das das Geld geht.",
  billingSettingsFirstRun:
    "Füllen Sie dies aus, bevor Sie etwas ausstellen. Es steht oben auf jedem Dokument, das Sie drucken, und dorthin werden Ihre Kunden um Zahlung gebeten.",
  billingSettingsIdentity: "Als wer Sie abrechnen",
  billingSettingsContact: "Wie Kunden Sie erreichen",
  billingSettingsBank: "Wohin das Geld geht",
  billingSettingsFooter: "Die Zeile unter den Summen",
  billingSettingsSaved:
    "Gespeichert. Jedes Dokument, das Sie ab jetzt drucken, trägt diese Angaben.",
  billingSettingsLoadFailed:
    "Ihre Rechnungsangaben konnten nicht geladen werden.",
  billingFieldLegalName: "Firmenname",
  billingLegalNameHint:
    "Der Name, unter dem Sie firmieren und Rechnungen stellen, wie eingetragen.",
  billingIssuerVatIdHint:
    "Leer lassen, wenn Sie keine USt-IdNr. haben. Der Ländercode steht voran.",
  billingFieldRegistrationNo: "Registernummer",
  billingRegistrationHint:
    "So, wie Ihr Register sie schreibt — HRB, KVK, SIREN, Companies House.",
  billingFieldPhone: "Telefon",
  billingFieldWebsite: "Website",
  billingFieldIban: "IBAN",
  billingIbanHint:
    "Wird vor dem Speichern gegen Länge und Prüfziffern Ihres Landes geprüft.",
  billingIbanPlaceholder: "BE68 5390 0754 7034",
  billingFieldBic: "BIC",
  billingBicPlaceholder: "KREDBEBB",
  billingBicHint: "Der internationale BIC- oder SWIFT-Code Ihrer Bank.",
  billingFieldBankName: "Bank",
  billingFieldAccountHolder: "Kontoinhaber",
  billingAccountHolderHint:
    "Nur wenn das Konto nicht auf Ihren Firmennamen läuft.",
  billingFieldFooterNote: "Fußzeile",
  billingFooterNoteHint:
    "Wird unter den Summen jedes Dokuments gedruckt — Eigentumsvorbehalt, Verzugsbedingungen, ein Dankeschön.",
  billingSettingsAccounting: "Die Währung Ihrer Buchführung",
  billingFieldBaseCurrency: "Buchungswährung",
  billingBaseCurrencyHint:
    "Sie können in jeder Währung fakturieren. Dies ist die Währung, in der Ihre MwSt.-Erklärung abgegeben wird — und in der die MwSt. einer Fremdwährungsrechnung zusätzlich gedruckt wird.",
  billingFxRates: "Wechselkurse",
  billingFxIntro:
    "Wer in einer anderen Währung fakturiert, braucht den veröffentlichten Kurs des Ausstellungstags. Die Kurse gehören Ihnen: Nichts wird für Sie abgerufen — womit Ihre Bücher umgerechnet werden, ist eine Datei, die Sie gewählt haben.",
  billingFxColDate: "Veröffentlicht",
  billingFxColRate: "Kurs je Euro",
  billingFxColSource: "Quelle",
  billingFxSourceEcb: "Referenzdatei",
  billingFxSourceManual: "Von Hand eingetragen",
  billingFxAdd: "Kurs hinzufügen",
  billingFxAddSaved: (currency: string, date: string) =>
    `${currency}-Kurs für ${date} gespeichert.`,
  billingFxRateHint:
    "Wie veröffentlicht: Einheiten dieser Währung je Euro, geschrieben 1,1626.",
  billingFxImport: "Kursdatei importieren",
  billingFxImportHint:
    "Fügen Sie die eurofxref-CSV der Europäischen Zentralbank ein, oder eine Datei in dieser Form. Eine Datei mit einem fehlerhaften Wert ändert nichts.",
  billingFxImportRun: "Importieren",
  billingFxImported: (rates: number, days: number) =>
    `${rates === 1 ? "1 Kurs" : `${rates} Kurse`} über ${days === 1 ? "1 Tag" : `${days} Tage`} importiert.`,
  billingFxEmpty:
    "Noch keine Kurse. Sie brauchen sie nur, wenn Sie in einer anderen Währung fakturieren.",
  billingFxLoadFailed: "Die Wechselkurse konnten nicht geladen werden.",
  billingDocumentFx: (rate: string, day: string) =>
    `Umgerechnet zu ${rate}, dem am ${day} veröffentlichten Referenzkurs.`,
  billingVatIn: (currency: string) => `MwSt. in ${currency}`,
  billingReportBaseCaption: (currency: string) => `Der Zeitraum in ${currency}`,
  billingReportBaseIntro: (currency: string) =>
    `Jedes Dokument oben, umgerechnet zu dem Kurs, der bei der Ausstellung darauf festgehalten wurde. Hieraus wird eine Erklärung in ${currency} erstellt.`,
  billingReportUnconverted: (count: number) =>
    count === 1
      ? "1 Dokument fehlt in diesen Zahlen: Für dieses wurde kein Wechselkurs gespeichert. Prüfen Sie es vor der Abgabe."
      : `${count} Dokumente fehlen in diesen Zahlen: Für sie wurde kein Wechselkurs gespeichert. Prüfen Sie sie vor der Abgabe.`,
  billingRemind: "Erinnern",
  billingRemindHint:
    "Schreiben Sie diesem Kunden eine Zahlungserinnerung und legen Sie sie in Entwürfe.",
  billingReminderDrafted: (
    invoice: string,
    outstanding: string,
    days: number,
  ) =>
    days === 1
      ? `Eine Erinnerung zu ${invoice} — ${outstanding} noch offen, 1 Tag überfällig — wartet in Entwürfe. Nichts wurde gesendet: Lesen Sie sie, ändern Sie, was Sie möchten, und senden Sie sie selbst.`
      : `Eine Erinnerung zu ${invoice} — ${outstanding} noch offen, ${days} Tage überfällig — wartet in Entwürfe. Nichts wurde gesendet: Lesen Sie sie, ändern Sie, was Sie möchten, und senden Sie sie selbst.`,
  billingReminderFailed:
    "Die Erinnerung konnte nicht geschrieben werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  billingNothingOverdue:
    "Nichts ist überfällig. Jede ausgestellte Rechnung ist entweder beglichen oder noch nicht fällig.",
  billingRecurring: "Wiederkehrend",
  billingRecurringTitle: "Wiederkehrende Rechnungen",
  billingRecurringChip: "Wiederkehrend",
  billingRecurringChipHint:
    "Eine wiederkehrende Rechnung hat diesen Entwurf erstellt.",
  billingNoSchedulesTitle: "Noch keine wiederkehrenden Rechnungen",
  billingNoSchedulesBody:
    "Richten Sie eine für alles ein, was Sie im Rhythmus abrechnen — eine Pauschale, ein Abo, eine Hosting-Gebühr. Jedes Mal, wenn sie fällig wird, erstellt alo einen Entwurf, den Sie prüfen und ausstellen.",
  billingNewSchedule: "Neue wiederkehrende Rechnung",
  billingScheduleFrom: "Diese Rechnung wiederholen",
  billingScheduleFromHint:
    "Richten Sie eine wiederkehrende Rechnung ein, die diese Positionen im Rhythmus erneut abrechnet. Jedes Auftreten erscheint als Entwurf — nie wird etwas für Sie ausgestellt.",
  billingScheduleName: "Name",
  billingScheduleNameHint:
    "Wie Sie diese Vereinbarung nennen. Wird nie auf die Rechnung gedruckt.",
  billingScheduleCadence: "Rechnet ab",
  billingCadenceWeekly: "Jede Woche",
  billingCadenceMonthly: "Jeden Monat",
  billingCadenceQuarterly: "Jedes Quartal",
  billingCadenceYearly: "Jedes Jahr",
  billingScheduleStart: "Erstmals am",
  billingScheduleEnd: "Bis",
  billingScheduleEndNever: "Kein Enddatum",
  billingScheduleNext: "Nächste",
  billingScheduleLast: "Zuletzt erstellt",
  billingScheduleRaised: "Erstellt",
  billingScheduleEach: "Jedes Mal",
  billingScheduleStatusActive: "Läuft",
  billingScheduleStatusPaused: "Pausiert",
  billingScheduleStatusEnded: "Beendet",
  billingScheduleStatusDue: "Fällig",
  billingSchedulePause: "Pausieren",
  billingScheduleResume: "Fortsetzen",
  billingScheduleDelete: "Löschen",
  billingScheduleDeleteTitle: "Diese wiederkehrende Rechnung löschen?",
  billingScheduleDeleteMessage:
    "Sie hört auf abzurechnen und verschwindet aus dieser Liste. Nur eine Vereinbarung, die nie einen Entwurf erstellt hat, lässt sich löschen — pausieren Sie eine, die es getan hat.",
  billingScheduleRunDue: "Fälliges erstellen",
  billingScheduleRunHint:
    "alo tut das jede Stunde von selbst. Dies ist nur für den Fall, dass Sie nicht warten möchten.",
  billingScheduleRunNone:
    "Nichts war fällig. Jede wiederkehrende Rechnung ist auf dem Stand.",
  billingScheduleRunDrafted: (count: number) =>
    count === 1
      ? "1 Entwurf wurde erstellt und wartet bei Ihren Rechnungen. Nichts wurde ausgestellt: Lesen Sie ihn, ändern Sie, was Sie möchten, und stellen Sie ihn selbst aus."
      : `${count} Entwürfe wurden erstellt und warten bei Ihren Rechnungen. Nichts wurde ausgestellt: Lesen Sie sie, ändern Sie, was Sie möchten, und stellen Sie sie selbst aus.`,
  billingScheduleSaved: (name: string) =>
    `„${name}“ ist eingerichtet. Jedes Mal, wenn sie fällig wird, erstellt alo einen Entwurf zur Prüfung.`,
  billingScheduleAnchorHint: (day: number) =>
    day > 28
      ? `Verankert am Tag ${day}: In einem kürzeren Monat wird am letzten Tag abgerechnet, im nächsten langen wieder am Tag ${day}.`
      : `Verankert am Tag ${day} des Monats.`,

  // CRM (tranche 5). German sales speech keeps the trade's loanwords — der
  // Deal, die Pipeline, das Board — and names a stage a Phase, as the German
  // CRM tools a salesperson has already used do. crmDocumentDraft returns a
  // capitalized noun, so both sentences that interpolate it ("Ihr … ist
  // bereit", "… erstellen") stay orthographically correct.
  crmBoard: "Board",
  crmList: "Liste",
  crmPipeline: "Pipeline",
  crmDeal: "Deal",
  crmStage: "Phase",
  crmStageArchived: "Archivierte Spalte",
  crmLoadFailed: "Ihre Deals konnten nicht geladen werden.",
  crmSaveFailed: "Die Änderung konnte nicht gespeichert werden.",
  crmDeleteFailed: "Das konnte nicht entfernt werden.",
  crmSuggestFailed: "Gerade konnten keine Unterhaltungen vorgeschlagen werden.",
  crmNoBoardTitle: "Noch keine Pipeline",
  crmNoBoardBody:
    "Jedes Board, das Sie hatten, wurde archiviert. Stellen Sie eines wieder her, um wieder an Deals zu arbeiten.",
  crmNoDealsTitle: "Noch keine Deals",
  crmNoDealsBody:
    "Legen Sie die erste Verkaufschance an und ziehen Sie sie über das Board, während sie vorankommt.",
  crmNoMatches: "Kein Deal entspricht Ihrer Eingabe.",
  crmNewDeal: "Neuer Deal",
  crmEditDeal: "Deal bearbeiten",
  crmEdit: "Bearbeiten",
  crmCreate: "Erstellen",
  crmSave: "Speichern",
  crmCancel: "Abbrechen",
  crmClose: "Schließen",
  crmDealSubtitle: "Worum es geht, mit wem, und was es wert ist.",
  crmFieldTitle: "Deal",
  crmFieldCompany: "Unternehmen",
  crmCompanyHint: "Das Unternehmen, wie Ihr ganzes Team es sehen soll.",
  crmFieldContactName: "Kontakt",
  crmFieldContactEmail: "Kontakt-E-Mail",
  crmContactEmailHint:
    "Wird genutzt, um die Unterhaltungen zu diesem Deal vorzuschlagen.",
  crmFieldValue: "Wert",
  crmValueHint: "Was der Deal wert ist, vor MwSt.",
  crmFieldCurrency: "Währung",
  crmCurrencyHint: "Drei Buchstaben, z. B. EUR.",
  crmFieldExpectedClose: "Erwarteter Abschluss",
  crmFieldSource: "Quelle",
  crmSourceHint:
    "Woher die Chance kam — eine Empfehlung, eine Kampagne, ein Anruf.",
  crmNotAnAmount: "Das ist kein Betrag.",
  crmDeleteDeal: "Löschen",
  crmDeleteDealConfirm:
    "Dies entfernt den Deal und alles, was darauf protokolliert wurde. Daraus erstellte Aufgaben bleiben in den Listen ihrer Besitzer. Es kann nicht rückgängig gemacht werden.",
  crmDealsTable: "Deals",
  crmDealFilters: "Deal-Filter",
  crmSearchDeals: "Deals durchsuchen",
  crmFilterStage: "Nach Phase filtern",
  crmFilterAnyStage: "Jede Phase",
  crmFilterState: "Nach Status filtern",
  crmFilterAnyState: "Jeder Status",
  crmFilterMine: "Nur meine",
  crmColDeal: "Deal",
  crmColCompany: "Unternehmen",
  crmColStage: "Phase",
  crmColValue: "Wert",
  crmColExpectedClose: "Erwarteter Abschluss",
  crmColState: "Status",
  crmStateOpen: "Offen",
  crmStateWon: "Gewonnen",
  crmStateLost: "Verloren",
  crmExpectedClose: (day: string) => `Erwartet ${day}`,
  crmLostBecause: (reason: string) => `Verloren: ${reason}`,
  crmLostTitle: "Warum ging er verloren?",
  crmLostMessage: (stage: string) =>
    `Wenn Sie diesen Deal nach „${stage}“ verschieben, wird er als verloren geschlossen. Sagen Sie warum, damit der Grund in Ihrem Gewonnen-verloren-Bericht erscheint.`,
  crmLostPlaceholder: "Preis, Timing, an einen Wettbewerber gegangen…",
  crmLostConfirm: "Als verloren markieren",
  crmLostReasonLabel: "Grund",
  crmLostReasonPrice: "Preis",
  crmLostReasonTiming: "Timing",
  crmLostReasonCompetitor: "Wettbewerber gewählt",
  crmLostReasonBudget: "Kein Budget",
  crmLostReasonNoDecision: "Keine Entscheidung",
  crmLostReasonNotAFit: "Passt nicht",
  crmRaiseQuote: "Angebot",
  crmRaiseInvoice: "Rechnung",
  crmCreateProject: "Projekt erstellen",
  crmCreateOpportunity: "Verkaufschance erstellen",
  crmMailOpportunityTitle: "Verkaufschance aus E-Mail erstellen",
  crmMailOpportunitySubtitle:
    "Prüfen Sie die Verkaufsdaten. Die vollständige Unterhaltung bleibt mit der Verkaufschance verknüpft.",
  crmMailOpportunityConfirm: "Verkaufschance erstellen",
  crmMailOpportunityLoadFailed: "Die Sales-Pipelines konnten nicht geladen werden.",
  crmMailOpportunityCreateFailed: "Die Verkaufschance konnte nicht erstellt werden.",
  crmMailConversation: "Quellunterhaltung",
  crmMailSource: "E-Mail",
  crmChoosePipeline: "Pipeline auswählen",
  crmChooseStage: "Phase auswählen",
  crmOpportunityCreated: "Verkaufschance erstellt und Unterhaltung verknüpft.",
  crmDeliveryProject: "Umsetzungsprojekt",
  crmOpenProject: "Projekt öffnen",
  crmProjectCreateTitle: "Umsetzungsprojekt starten",
  crmProjectCreateSubtitle:
    "Prüfen Sie das Projekt vor dem Erstellen. Die gewonnene Chance und die Umsetzung bleiben in beiden Apps verknüpft.",
  crmProjectCreateSummary: (deal: string) => `Umsetzung aus „${deal}“ erstellen.`,
  crmProjectName: "Projektname",
  crmProjectCreateConfirm: "Projekt erstellen",
  crmProjectCreateFailed: "Das Projekt konnte nicht erstellt werden.",
  crmDocumentDraft: (kind: string): string =>
    kind === "invoice" ? "Rechnungsentwurf" : "Angebotsentwurf",
  crmDocumentQuote: "Angebotsentwurf",
  crmDocumentInvoice: "Rechnungsentwurf",
  crmRelatedBilling: "Zugehörige Abrechnung",
  crmRelatedBillingEmpty: "Aus dieser Verkaufschance wurden noch keine Abrechnungsdokumente erstellt.",
  crmRelatedBillingLoadFailed: "Zugehörige Abrechnungsdokumente konnten nicht geladen werden.",
  crmRaiseTitle: (document: string) => `${document} erstellen`,
  crmRaiseSubtitle:
    "Er landet als Entwurf unter Rechnungen, damit Sie ihn prüfen und vervollständigen. Nichts wird ausgestellt, nichts wird gesendet.",
  crmRaiseFrom: (deal: string, value: string) =>
    `Aus „${deal}“, im Wert von ${value}.`,
  crmRaiseConfirm: "Erstellen",
  crmRaiseFailed: "Das Dokument konnte nicht erstellt werden.",
  crmFieldVatRate: "MwSt.-Satz",
  crmVatRateHint:
    "Der Satz, zu dem diese Position berechnet wird, in Prozent — z. B. 21.",
  crmFieldCountry: "Land des Kunden",
  crmCountryHint:
    "Zwei Buchstaben. Dieser Deal ist noch ein Lead, daher wird daraus ein Kunde angelegt — und das Land entscheidet über die MwSt.-Behandlung.",
  crmRaisedTitle: (document: string) => `Ihr ${document} ist bereit`,
  crmRaisedSubtitle:
    "Öffnen Sie ihn unter Rechnungen und prüfen Sie Positionen, Adresse und MwSt.",
  crmRaisedWorth: (gross: string) => `${gross} inklusive MwSt.`,
  crmOpenInBilling: "In Rechnungen öffnen",
  crmReport: "Bericht",
  crmReportPeriod: "Berichtszeitraum",
  crmReportFrom: "Von",
  crmReportTo: "Bis",
  crmReportShow: "Anzeigen",
  crmReportThisQuarter: "Dieses Quartal",
  crmReportLastQuarter: "Letztes Quartal",
  crmReportCustom: "Benutzerdefinierter Zeitraum",
  crmReportQuickRanges: "Schnellauswahl",
  crmReportToday: "Heute",
  crmReportYesterday: "Gestern",
  crmReportLast7Days: "Letzte 7 Tage",
  crmReportLast28Days: "Letzte 28 Tage",
  crmReportLast30Days: "Letzte 30 Tage",
  crmReportApply: "Anwenden",
  crmReportDownloadCsv: "CSV herunterladen",
  crmReportDownloadFailed: "Der Bericht konnte nicht heruntergeladen werden.",
  crmReportBasis: (from: string, to: string) =>
    `Gewonnen und verloren zwischen ${from} und ${to}.`,
  crmReportOpenAsOf: (at: string) =>
    `Die offene Pipeline zeigt den Stand vom ${at}.`,
  crmReportOpenCaption: (currency: string) =>
    `Offene Pipeline nach Phase (${currency})`,
  crmReportClosedCaption: (currency: string) =>
    `Im Zeitraum geschlossen (${currency})`,
  crmReportColDeals: "Deals",
  crmReportOpenTotal: "Offen gesamt",
  crmReportWinRateLabel: "Erfolgsquote",
  crmReportClosedDeals: "geschlossene Deals",
  crmReportWinRate: (rate: string, won: number, closed: number) =>
    `Erfolgsquote ${rate} — ${won} von ${closed} geschlossenen Deals.`,
  crmReportNoWinRate:
    "In diesem Zeitraum wurde kein Deal geschlossen, daher gibt es keine Erfolgsquote.",
  crmReportEmptyTitle: "Noch nichts zu berichten",
  crmReportEmptyBody:
    "Dieses Board hält keine Deals. Legen Sie einen an, und er erscheint hier — nach Phase und nach Währung.",
  crmActivityTitle: "Protokoll",
  crmActivityKind: "Art des Eintrags",
  crmActivityPlaceholder: "Was besprochen oder vereinbart wurde…",
  crmActivityAdd: "Protokollieren",
  crmActivityDelete: "Eintrag löschen",
  crmActivityEmpty: "Noch nichts protokolliert.",
  crmKindNote: "Notiz",
  crmKindCall: "Anruf",
  crmKindMeeting: "Meeting",
  crmNextStepsTitle: "Nächste Schritte",
  crmNextStepPlaceholder: "Was als Nächstes passiert…",
  crmNextStepDue: "Fällig",
  crmNextStepAdd: "Hinzufügen",
  crmNextStepsEmpty: "Noch kein nächster Schritt vereinbart.",
  crmOpenInTasks: "In Aufgaben öffnen",
  crmThreadsTitle: "Unterhaltungen",
  crmThreadsEmpty: "Noch keine Unterhaltung verknüpft.",
  crmThreadSuggest: "Unterhaltungen vorschlagen",
  crmThreadLink: "Verknüpfen",
  crmThreadUnlink: "Trennen",
  crmThreadOpenInMail: "In E-Mail öffnen",
  crmThreadNotYours:
    "Diese Unterhaltung liegt nicht in Ihrem Postfach — fragen Sie die Person, die sie verknüpft hat.",
  crmThreadLinkedBy: (who: string, when: string) =>
    `Verknüpft von ${who} · ${when}`,
  crmSuggestionsEmpty:
    "Nichts in Ihren letzten E-Mails passt zu den Adressen dieses Deals.",
  crmSuggestionAddress: (address: string) => `Passt zu ${address}`,
  crmSuggestionDomain: (address: string) =>
    `Gleiches Unternehmen wie ${address}`,

  // Insights (tranche 5). Chart titles here must say the same words the
  // server seeds a tenant's overview with (`insights_gallery.rs`, DE table)
  // — if the two drift, a pinned chart looks like a different chart. The
  // German calendar week is "KW", and quarters keep "Q".
  insightsBoards: "Boards",
  insightsLoadFailed: "Ihre Boards konnten nicht geladen werden.",
  insightsBoardLoadFailed: "Dieses Board konnte nicht geladen werden.",
  insightsFiguresFailed: "Diese Zahlen konnten nicht gelesen werden.",
  insightsSaveFailed: "Die Änderung konnte nicht gespeichert werden.",
  insightsDeleteFailed: "Das konnte nicht entfernt werden.",
  insightsNewBoard: "Neues Board",
  insightsBoardNamePrompt: "Wie soll dieses Board heißen?",
  insightsBoardNamePlaceholder: "Liquidität",
  insightsRenameBoard: "Umbenennen",
  insightsDeleteBoard: "Board löschen",
  insightsDeleteBoardConfirm: (name: string) =>
    `Das Board „${name}“ löschen? Seine Diagramme gehen mit — die Rechnungen und Deals dahinter bleiben.`,
  insightsRefresh: "Zahlen aktualisieren",
  insightsNoBoardsTitle: "Noch keine Boards",
  insightsNoBoardsBody:
    "Ein Board hält die Zahlen, die Sie auf einen Blick sehen wollen — was Sie berechnet haben, was man Ihnen schuldet, was in der Pipeline ist.",
  insightsNoTilesTitle: "Nichts an dieses Board geheftet",
  insightsNoTilesBody:
    "Diagramme, die an dieses Board geheftet werden, erscheinen hier.",
  insightsAddChart: "Diagramm hinzufügen",
  insightsGalleryTitle: "Fertige Diagramme",
  insightsGallerySubtitle:
    "Wählen Sie eines, um es an dieses Board zu heften. Umbenennen oder entfernen können Sie es danach.",
  insightsGalleryClose: "Schließen",
  insightsGalleryLoadFailed:
    "Die fertigen Diagramme konnten nicht geladen werden.",
  insightsGalleryRevenueByMonth: "Umsatz nach Monat",
  insightsGalleryRevenueByMonthBody:
    "Was Sie fakturiert haben, Monat für Monat über das letzte Jahr — ohne MwSt.",
  insightsGalleryOutstanding: "Offene Forderungen",
  insightsGalleryOutstandingBody:
    "Alles, was man Ihnen auf ausgestellte Rechnungen noch schuldet, als eine Zahl.",
  insightsGalleryOverdueAging: "Überfällig nach Alter",
  insightsGalleryOverdueAgingBody:
    "Was geschuldet wird, gruppiert nach Verzug: 0–30, 31–60, 61–90 und über 90 Tage.",
  insightsGalleryVatByQuarter: "MwSt. nach Quartal",
  insightsGalleryVatByQuarterBody:
    "Berechnete MwSt. je Quartal — die Form, in der eine Erklärung abgegeben wird.",
  insightsGalleryTopCustomers: "Top-Kunden",
  insightsGalleryTopCustomersBody:
    "Von wem der Umsatz dieses Jahres kam, die größten zehn zuerst.",
  insightsGalleryPaymentsByMonth: "Zahlungseingänge",
  insightsGalleryPaymentsByMonthBody:
    "Geld, das tatsächlich angekommen ist, Monat für Monat, in der Währung, in der es ankam.",
  insightsGalleryPipelineByStage: "Pipeline nach Phase",
  insightsGalleryPipelineByStageBody:
    "Der Wert offener Deals in jeder Spalte Ihres Trichters.",
  insightsGalleryWonThisMonth: "Diesen Monat gewonnen",
  insightsGalleryWonThisMonthBody:
    "Der Wert der in diesem Monat als gewonnen geschlossenen Deals.",
  insightsGalleryWinRateByQuarter: "Erfolgsquote nach Quartal",
  insightsGalleryWinRateByQuarterBody:
    "Wie oft ein entschiedener Deal gewonnen wurde, Quartal für Quartal.",
  insightsGalleryWonByMonth: "Gewonnen nach Monat",
  insightsGalleryWonByMonthBody:
    "Gewonnener Deal-Wert, Monat für Monat über das letzte Jahr.",
  insightsAsk: "Diagramm erfragen",
  insightsAskSubtitle:
    "Beschreiben Sie, was Sie sehen möchten. Sie bekommen das Diagramm zuerst zum Ansehen — dem Board wird nichts hinzugefügt, bis Sie es anheften.",
  insightsAskLabel: "Ihre Frage",
  insightsAskPlaceholder:
    "Wie viel haben wir dieses Jahr pro Monat fakturiert?",
  insightsAskSubmit: "Fragen",
  insightsAskClose: "Schließen",
  insightsAskPreview: "Das vorgeschlagene Diagramm",
  insightsAskPin: "An dieses Board heften",
  insightsAskDiscard: "Verwerfen",
  insightsAskRepaired:
    "Der erste Versuch passte nicht zu den Daten und wurde vor dem Zeichnen korrigiert.",
  insightsAskFailed: "Aus dieser Frage ließ sich kein Diagramm bauen.",
  insightsAskUnavailable:
    "Der Assistent ist für diesen Arbeitsbereich nicht eingeschaltet.",
  insightsTileActions: (title: string) => `Optionen für ${title}`,
  insightsRenameTile: "Diagramm umbenennen",
  insightsRenameTilePrompt: "Wie soll dieses Diagramm heißen?",
  insightsRemoveTile: "Diagramm entfernen",
  insightsRemoveTileConfirm: (title: string) =>
    `„${title}“ von diesem Board entfernen? Die gezählten Datensätze bleiben unberührt.`,
  insightsWiden: "Breiter machen",
  insightsNarrow: "Schmaler machen",
  insightsMoveLeft: "Nach vorn verschieben",
  insightsMoveRight: "Nach hinten verschieben",
  insightsUnreadableTitle: "Mit einer neueren Version von alo erstellt",
  insightsUnreadableBody:
    "Die Frage dieses Diagramms kann hier nicht gelesen werden, daher werden seine Zahlen nicht angezeigt.",
  insightsNoFigures: "Für diesen Zeitraum gibt es nichts zu zeigen.",
  insightsTruncated:
    "Nur die größten Kategorien werden gezeigt; der Rest ist als „Sonstige“ zusammengefasst.",
  insightsNoteUnconverted: (count: number) =>
    count === 1
      ? "1 Dokument konnte nicht in Ihre Buchungswährung umgerechnet werden und wird nicht mitgezählt."
      : `${count} Dokumente konnten nicht in Ihre Buchungswährung umgerechnet werden und werden nicht mitgezählt.`,
  insightsColBucket: "Gruppe",
  insightsColValue: "Wert",
  insightsBucketTotal: "Gesamt",
  insightsBucketOther: "Sonstige",
  insightsGroupAll: "Alle",
  insightsValueNone: "Keine",
  insightsValueUnknown: "Unbekannt",
  insightsStatusIssued: "Ausgestellt",
  insightsStatusPaid: "Bezahlt",
  insightsOutcomeWon: "Gewonnen",
  insightsOutcomeLost: "Verloren",
  insightsOutcomeOpen: "Offen",
  insightsAgeNotDue: "Noch nicht fällig",
  insightsAge0To30: "0–30 Tage",
  insightsAge31To60: "31–60 Tage",
  insightsAge61To90: "61–90 Tage",
  insightsAge90Plus: "Über 90 Tage",
  insightsQuarter: (quarter: number, year: number) => `Q${quarter} ${year}`,
  insightsWeek: (week: number, year: number) => `KW ${week} ${year}`,

  // Projekte (tranche 6). Die Wörter der Kundenarbeit: das Engagement, die
  // Stunden darauf, die Woche, als die sie eingereicht werden, und die
  // Entscheidung über diese Woche. Dauern schreibt der Katalog, wie man sie
  // sagt („7 Std. 30 Min.“), nie als Dezimalstunden; „abrechenbar“ ist das
  // eine Wort für billable, und die zurückgewiesene Woche trägt das Wort des
  // Verlaufs (auditActionReject), wie es die Statuschip-Regel verlangt.
  projectsTabList: "Alle Projekte",
  projectsTabMyWork: "Meine Arbeit",
  projectsWorkspaceTasks: "Aufgaben",
  projectsTabWeek: "Stundenzettel",
  projectsTabApprovals: "Genehmigungen",
  projectsTabReports: "Berichte",
  projectsTabPlan: "Zeitleiste",
  projectsLoadFailed: "Ihre Projekte konnten nicht geladen werden.",
  projectsSalesOrigin: "In Sales gewonnen",
  projectsOpenSalesOrigin: "Verkaufschance in Sales öffnen",
  projectsSalesOriginLoadFailed: "Die ursprüngliche Verkaufschance konnte nicht geladen werden.",
  projectsResources: "Projektarbeitsbereich",
  projectsResourcesSubtitle: "Dateien, Unterhaltung, Kickoff und Startaufgaben für dieses Projekt.",
  projectsSetupAction: "Arbeitsbereich einrichten",
  projectsSetupAddAction: "Ressourcen hinzufügen",
  projectsSetupTitle: "Projektarbeitsbereich einrichten",
  projectsSetupSubtitle: (project: string) => `Prüfen Sie, was Alo für „${project}“ erstellen soll.`,
  projectsSetupConfirm: "Ausgewählte Ressourcen erstellen",
  projectsSetupFiles: "Gemeinsame Projektdateien",
  projectsSetupFilesDetail: "Einen Drive-Bereich für Projektdokumente und Assets erstellen.",
  projectsSetupChat: "Projektunterhaltung",
  projectsSetupChatDetail: "Einen mandantenweit sichtbaren Chatraum für die Umsetzung erstellen.",
  projectsSetupTasks: "Startaufgaben",
  projectsSetupTasksDetail: "Aufgaben für Umfang, Kickoff und Lieferplan hinzufügen.",
  projectsSetupKickoff: "Kickoff-Meeting",
  projectsSetupKickoffDetail: "Einen einstündigen Kickoff mit Erinnerung zur Agenda hinzufügen.",
  projectsSetupKickoffTime: "Kickoff beginnt",
  projectsSetupReviewNote: "Bis zur Bestätigung wird nichts erstellt. Wiederholungen ergänzen nur fehlende Ressourcen.",
  projectsSetupTaskScope: "Lieferumfang bestätigen",
  projectsSetupTaskKickoff: "Projekt-Kickoff vorbereiten",
  projectsSetupTaskPlan: "Lieferplan veröffentlichen",
  projectsSetupFailed: "Die ausgewählten Projektressourcen konnten nicht erstellt werden.",
  projectsSetupLoadFailed: "Projektressourcen konnten nicht geladen werden.",
  projectsFiles: "Projektdateien",
  projectsChatRoom: "Projektchat",
  projectsKickoffMeeting: "Kickoff-Meeting",
  projectsStarterTasks: (count: number) => `${count} Startaufgaben`,
  projectsWorkspaceLoadFailed: "Dieses Projekt konnte nicht geöffnet werden.",
  projectsWorkspaceUnavailable: "Projekt nicht verfügbar",
  projectsRetry: "Erneut versuchen",
  projectsSaveFailed: "Die Änderung konnte nicht gespeichert werden.",
  projectsStartFailed: "Der Timer konnte nicht gestartet werden.",
  projectsStopFailed: "Der Timer konnte nicht angehalten werden.",
  projectsCancel: "Abbrechen",
  projectsSave: "Speichern",
  projectsEdit: "Bearbeiten",
  projectsOpenProject: (name: string) => `${name} öffnen`,
  projectsDetailsTitle: "Projektdetails",
  projectsDetailsSubtitle:
    "Halten Sie Ergebnis, Zeitplan und aktuellen Stand für alle nachvollziehbar fest.",
  projectsDescription: "Beschreibung",
  projectsStatus: "Status",
  projectsStatusPlanned: "Geplant",
  projectsStatusActive: "Aktiv",
  projectsStatusOnHold: "Pausiert",
  projectsStatusCompleted: "Abgeschlossen",
  projectsStatusCancelled: "Abgebrochen",
  projectsTargetOn: "Zieldatum",
  projectsDatesInvalid: "Das Zieldatum kann nicht vor dem Startdatum liegen.",
  projectsActions: "Aktionen",
  projectsNew: "Neues Projekt",
  projectsNewTitle: "Projekt erstellen",
  projectsNewSubtitle:
    "Benennen Sie die Arbeit und entscheiden Sie, für wen sie ist.",
  projectsName: "Projektname",
  projectsNamePlaceholder: "Zum Beispiel Website-Relaunch",
  projectsWorkType: "Diese Arbeit ist für",
  projectsClientWork: "Einen Kunden",
  projectsInternalWork: "Unser Unternehmen",
  projectsClientWorkHint: "Diese Arbeit einem Kunden berechnen",
  projectsInternalWorkHint: "Diese Arbeit intern behalten",
  projectsNewCustomerHint:
    "Sätze und Budgets können Sie nach dem Erstellen des Projekts hinzufügen.",
  projectsCreate: "Projekt erstellen",
  projectsCreateFailed: "Das Projekt konnte nicht erstellt werden.",

  // Dauern und Sätze. `projectsNoTime` ist der Strich einer leeren Zelle.
  projectsNoTime: "—",
  projectsHoursShort: (hours: number) => `${hours} Std.`,
  projectsMinutesShort: (minutes: number) => `${minutes} Min.`,
  projectsPerHour: (amount: string) => `${amount}/Std.`,
  projectsPercent: (percent: number) => `${percent} %`,
  projectsUnpriced: "Nicht bewertet",

  // Die Projektliste.
  projectsProject: "Projekt",
  projectsAllProjects: "Alle Projekte",
  projectsCustomer: "Kunde",
  projectsCustomerHint:
    "Der Kunde, dem die Stunden dieses Projekts in Rechnung gestellt werden.",
  projectsCustomerPick: "Kunden wählen…",
  projectsNoCustomersAvailable:
    "Es gibt noch keine Kunden. Legen Sie zuerst unter Rechnungen einen an.",
  projectsCustomerUnknown: "Unbekannter Kunde",
  projectsInternal: "Intern",
  projectsRate: "Stundensatz",
  projectsRateHint:
    "Bleibt er leer, werden die Stunden gezählt, aber nicht bewertet.",
  projectsRateInvalid: "Schreiben Sie den Satz als Betrag, zum Beispiel 95,00.",
  projectsHoursLogged: "Stunden",
  projectsBillableHours: "Abrechenbar",
  projectsOfWhichBillable: (duration: string) => `${duration} abrechenbar`,
  projectsBudget: "Budget",
  projectsHealth: "Projektlage",
  projectsHealthOnTrack: "Auf Kurs",
  projectsHealthAtRisk: "Braucht Aufmerksamkeit",
  projectsHealthNeedsTarget:
    "Setzen Sie ein Zieldatum, damit das Lieferrisiko sichtbar wird.",
  projectsUpdates: "Projekt-Updates",
  projectsUpdatesSubtitle:
    "Teilen Sie Fortschritt, Entscheidungen und Risiken mit allen, die diesem Projekt folgen.",
  projectsUpdateHealth: "Lage aktualisieren",
  projectsUpdateOffTrack: "Nicht auf Kurs",
  projectsUpdatePlaceholder:
    "Was hat sich geändert? Ergebnis, Entscheidung, Risiko oder nächster Schritt.",
  projectsUpdateHint:
    "Kurz und nützlich für jemanden, der später aufholen muss.",
  projectsPublishUpdate: "Update veröffentlichen",
  projectsUpdatesEmpty: "Noch keine Updates",
  projectsUpdatesEmptyBody:
    "Veröffentlichen Sie das erste Update und geben Sie diesem Projekt eine nachlesbare Geschichte.",
  projectsUpdatesLoadFailed:
    "Die Projekt-Updates konnten nicht geladen werden.",
  projectsUpdateSaveFailed: "Das Update konnte nicht veröffentlicht werden.",
  projectsRemoveAttachment: "Anhang entfernen",
  projectsSomeone: "Jemand",
  projectsBlockedTasks: (count: number) =>
    count === 1 ? "1 blockierte Aufgabe" : `${count} blockierte Aufgaben`,
  projectsOverdueTasks: (count: number) =>
    count === 1 ? "1 überfällige Aufgabe" : `${count} überfällige Aufgaben`,
  projectsWorkload: "Auslastung",
  projectsWorkloadEmpty: "Noch keine offene Arbeit zugewiesen.",
  projectsOpenTasks: (count: number) =>
    count === 1 ? "1 offene Aufgabe" : `${count} offene Aufgaben`,
  projectsBudgetUsed: "Budget verbraucht",
  projectsBudgetHours: "Budget (Stunden)",
  projectsBudgetAmount: "Budget (Betrag)",
  projectsBudgetHint:
    "Nur zur Orientierung. Nichts verhindert eine Stunde darüber hinaus.",
  projectsBudgetHoursInvalid: "Schreiben Sie das Budget als ganze Stundenzahl.",
  projectsBudgetAmountInvalid:
    "Schreiben Sie das Budget als Betrag, zum Beispiel 7600,00.",
  projectsLastWorked: "Zuletzt gearbeitet",
  projectsNeverWorked: "Nie",
  projectsStartsOn: "Beginnt am",
  projectsMakeClientWork: "Zu Kundenarbeit machen",
  projectsStartTimerOn: (project: string) => `Timer auf ${project} starten`,
  projectsStartTimer: "Timer starten",
  projectsEmptyTitle: "Noch keine Projekte",
  projectsEmptyBody:
    "Legen Sie ein Projekt für Kundenarbeit oder für das eigene Unternehmen an und erfassen Sie dann Zeit darauf.",

  // Das Engagement-Formular.
  projectsClientSubtitle:
    "Für wen dieses Projekt gearbeitet wird und was eine Stunde darauf wert ist.",
  projectsPersonalBoard:
    "Das ist ein persönliches Board. Nur ein Teamprojekt kann Kundenarbeit sein — seine Stunden genehmigt jemand anderes, und sie werden einem Kunden in Rechnung gestellt.",
  projectsDetach: "Intern machen",
  projectsDetachTitle: "Diese Arbeit intern machen?",
  projectsDetachBody:
    "Die Stunden bleiben genau, wie sie sind. Was wegfällt, ist der Anspruch, sie einem Kunden zu berechnen — und Stunden, die schon auf einer Rechnung stehen, behalten diese Rechnung.",

  // Das Wochenraster.
  projectsPreviousWeek: "Vorherige",
  projectsNextWeek: "Nächste",
  projectsThisWeek: "Diese Woche",
  projectsWeekOf: (from: string, to: string) => `${from} – ${to}`,
  projectsBillableOf: (hours: string) => `${hours} abrechenbar`,
  projectsWeek: "Woche",
  projectsDay: "Tag",
  projectsTask: "Aufgabe",
  projectsDuration: "Dauer",
  projectsDurationHint:
    "90, 1:30 und 1,5 bedeuten alle anderthalb Stunden. 2h bedeutet zwei Stunden.",
  projectsDurationInvalid:
    "Schreiben Sie eine Dauer wie 90, 1:30, 1,5 oder 2h — höchstens einen Tag.",
  projectsTotal: "Gesamt",
  projectsAddRow: "Projektzeile hinzufügen…",
  projectsBillable: "Für den Kunden abrechenbar",
  projectsNotBillable: "nicht abrechenbar",
  projectsNote: "Notiz",
  projectsNoNote: "Keine Notiz",
  projectsNoteHint:
    "Woran Sie gearbeitet haben. Niemand außerhalb dieses Arbeitsbereichs liest das.",
  projectsProposedEntry: "vorgeschlagen",
  projectsBilledEntry: "auf einer Rechnung",
  projectsReadyToInvoice: "Bereit zur Abrechnung",
  projectsReadyToInvoiceBody: (duration: string) =>
    `${duration} genehmigte Zeit ist noch nicht abgerechnet.`,
  projectsWorkflowEyebrow: "Nächster Schritt",
  projectsWorkflowLabel: "Projektablauf",
  projectsWorkflowTasks: "Aufgaben",
  projectsWorkflowTime: "Zeit",
  projectsWorkflowApproval: "Genehmigung",
  projectsWorkflowInvoice: "Rechnung",
  projectsWorkflowTasksTitle: "Die Arbeit festlegen",
  projectsWorkflowTasksBody:
    "Legen Sie die erste Aufgabe an, damit das Team weiß, was als Nächstes ansteht.",
  projectsWorkflowTimeTitle: "Die Arbeit erfassen",
  projectsWorkflowTimeBody:
    "Erfassen Sie Zeit auf dieses Projekt oder seine Aufgaben, solange die Arbeit frisch ist.",
  projectsWorkflowApprovalTitle: "Die Zeit zur Genehmigung einreichen",
  projectsWorkflowApprovalBody:
    "Prüfen Sie die Woche und reichen Sie sie ein, damit genehmigte Kundenarbeit abgerechnet werden kann.",
  projectsWorkflowAwaitingApprovalTitle: "Zeit wartet auf Genehmigung",
  projectsWorkflowAwaitingApprovalBody:
    "Diese Zeit ist bereits eingereicht. Prüfen Sie den Stundenzettel oder warten Sie vor der Abrechnung auf die Genehmigung.",
  projectsWorkflowInvoiceTitle: "Aus genehmigter Arbeit eine Rechnung machen",
  projectsWorkflowContinueTitle: "Das Projekt in Bewegung halten",
  projectsWorkflowContinueBody:
    "Erfassen Sie den nächsten Zeiteintrag, während die Arbeit weitergeht.",
  projectsReviewTimesheet: "Stundenzettel prüfen",
  projectsCreateInvoice: "Rechnung erstellen",
  projectsCreateInvoiceSubtitle:
    "Wählen Sie die genehmigte Zeit, die in einen neuen Rechnungsentwurf übernommen wird.",
  projectsInvoiceThrough: "Abrechnen bis",
  projectsInvoiceCutoffHint:
    "Nur genehmigte, noch nicht abgerechnete Zeit bis zu diesem Tag wird aufgenommen.",
  projectsNothingToInvoice: "Nichts bereit zur Abrechnung",
  projectsNothingToInvoiceBody:
    "Genehmigte Zeit erscheint hier, sobald die Woche genehmigt ist.",
  projectsUnratedTime: "Für diese Zeit ist kein Stundensatz festgelegt",
  projectsInvoiceRate: (rate: string) => `${rate} pro Stunde`,
  projectsBelgianVat:
    "Auf diesen Entwurf wird der belgische Regelsatz der MwSt. angewendet.",
  projectsCreateDraftInvoice: "Rechnungsentwurf erstellen",
  projectsInvoiceLoadFailed: "Die genehmigte Zeit konnte nicht geladen werden.",
  projectsInvoiceCreateFailed:
    "Der Rechnungsentwurf konnte nicht erstellt werden.",
  projectsCellLabel: (project: string, day: string, duration: string) =>
    `${project}, ${day}: ${duration}`,
  projectsDeleteEntry: "Löschen",
  projectsDeleteEntryTitle: "Diese Stunden löschen?",
  projectsDeleteEntryBody:
    "Der Eintrag ist dann endgültig weg. Dafür muss seine Woche offen sein.",
  projectsWeekEmptyTitle: "Diese Woche ist nichts erfasst",
  projectsWeekEmptyBody:
    "Erfassen Sie Ihren ersten Zeiteintrag. Wählen Sie ein Projekt, geben Sie Dauer und Notiz ein, und er erscheint in dieser Wochenübersicht.",
  projectsWeekTitle: "Wöchentlicher Stundenzettel",
  projectsWeekEntriesLabel: "Zeiteinträge dieser Woche",
  projectsWeekPurpose:
    "Erfassen Sie Ihre Arbeit, prüfen Sie die Woche und reichen Sie sie dann zur Genehmigung ein.",
  projectsWeekAllScope: "Ihre ganze Woche über alle Projekte.",
  projectsWeekProjectScope: (project: string) =>
    `Zeit für ${project}. Eingereicht wird trotzdem Ihre ganze Woche.`,
  projectsAddTime: "Zeit erfassen",
  projectsChooseTimeProject: "Woran haben Sie gearbeitet?",
  projectsChooseTimeProjectHint:
    "Wählen Sie ein Projekt, um für diese Woche einen Zeiteintrag zu erfassen.",
  projectsBillableOfWeek: (duration: string) => `${duration} abrechenbar`,
  projectsCompleteWeek: "Ganze Woche",
  projectsCompleteWeekSubmission: "Ganze Woche zur Genehmigung eingereicht",
  projectsProposedInWeek: (duration: string) =>
    `${duration} vorgeschlagen, noch nicht angenommen`,
  // Über einen Vorschlag entscheiden: erst das Annehmen macht ihn zur Stunde.
  projectsAcceptEntry: "Annehmen",
  projectsRejectEntry: "Verwerfen",
  projectsAcceptEntryLabel: (project: string, duration: string) =>
    `Vorschlag über ${duration} auf ${project} annehmen`,
  projectsRejectEntryLabel: (project: string, duration: string) =>
    `Vorschlag über ${duration} auf ${project} verwerfen`,
  projectsSuggestionsWaiting: (count: number) =>
    count === 1
      ? "1 Vorschlag wartet diese Woche auf Sie."
      : `${count} Vorschläge warten diese Woche auf Sie.`,
  projectsSubmitWeek: "Woche einreichen",
  projectsWithdrawWeek: "Zurücknehmen",
  projectsRejectedBecause: (note: string) => `Zurückgewiesen: ${note}`,

  // Der Plan — Meilensteine auf einer Zeitachse. „Erreicht“ ist bewusst das
  // Wort eines Menschen, nie „abgeschlossen“: ein Meilenstein ist erreicht,
  // wenn jemand sagt, das Ergebnis wurde angenommen.
  projectsPlanLoadFailed: "Der Plan konnte nicht geladen werden.",
  projectsMilestoneAdd: "Meilenstein hinzufügen",
  projectsMilestoneNew: "Neuer Meilenstein",
  projectsMilestoneName: "Meilenstein",
  projectsMilestoneNameHint:
    "Wofür der Termin steht — „Design abgenommen“, „Beta beim Pilotkunden“.",
  projectsMilestoneDue: "Datum",
  projectsMilestoneDueHint:
    "Der Tag, an dem er fällig ist. Ihn zu verschieben ist normal; nichts wird dadurch angehalten.",
  projectsMilestoneReach: "Als erreicht markieren",
  projectsMilestoneReopen: "Doch noch nicht erreicht",
  projectsMilestoneReached: "Erreicht",
  projectsMilestoneLate: "Überfällig",
  projectsMilestoneNoTasks: "Noch keine Aufgaben darunter",
  projectsMilestoneTasksClosed: (done: number, total: number) =>
    `${done} von ${total} Aufgaben geschlossen`,
  projectsMilestoneDelete: "Löschen",
  projectsMilestoneDeleteTitle: "Diesen Meilenstein löschen?",
  projectsMilestoneDeleteBody:
    "Der Termin fällt weg; die Aufgaben darunter bleiben genau, wo sie auf dem Board sind.",
  projectsPlanUnplaced: "Nicht im Plan",
  projectsPlanPlace: "Einordnen unter…",
  projectsPlanPlaceTask: (task: string) =>
    `${task} unter einen Meilenstein stellen`,
  projectsPlanRemove: "Herausnehmen",
  projectsPlanEmptyTitle: "Noch kein Plan",
  projectsPlanEmptyBody:
    "Ein Meilenstein ist ein benannter Termin in diesem Projekt — die Termine, nach denen ein Kunde fragt. Legen Sie den ersten an und stellen Sie dann die Aufgaben des Boards darunter.",
  projectsTimelineAllEmptyTitle: "Keine Meilensteine in Ihren Projekten",
  projectsTimelineAllEmptyBody:
    "Wählen Sie oben ein Projekt, um seinen ersten Meilenstein anzulegen, oder lassen Sie diese Ansicht auf Alle Projekte für die Zeitleiste des Portfolios.",

  // Vorlagen: ein wiederverwendbares Board und die Kopie, die davon startet.
  projectsTemplateNew: "Neu aus Vorlage",
  projectsTemplateNewTitle: "Aus einer Vorlage starten",
  projectsTemplateNewSubtitle: "Die Form der Arbeit, auf neuen Terminen",
  projectsTemplateCreate: "Projekt erstellen",
  projectsTemplateWhich: "Vorlage",
  projectsTemplateWhichHint:
    "Karten, Spalten, Checklisten und Labels kommen mit — nicht Zuständige, Kommentare, Stunden oder erledigte Karten.",
  projectsTemplateOption: (name: string, tasks: number, milestones: number) =>
    `${name} — ${tasks} ${tasks === 1 ? "Karte" : "Karten"}, ${milestones} ${
      milestones === 1 ? "Meilenstein" : "Meilensteine"
    }`,
  projectsTemplateName: "Name des neuen Projekts",
  projectsTemplateNameHint: "So heißt es auf dem Board.",
  projectsTemplateStarts: "Beginnt am",
  projectsTemplateStartsHint:
    "Der erste Meilenstein der Vorlage landet hier; alle anderen Termine behalten ihren Abstand.",
  projectsTemplateCustomerHint:
    "Eine Vorlage ist eine Form, kein Kunde. Für interne Arbeit leer lassen; Satz und Budget kommen so oder so mit.",
  projectsTemplateNoCustomer: "Interne Arbeit",
  projectsTemplateNoPlan:
    "Diese Vorlage hat keine Meilensteine, ihre Termine werden also genau so übernommen, wie sie sind.",
  projectsTemplateMarkOn: (project: string) => `${project} zur Vorlage machen`,
  projectsTemplateUnmarkOn: (project: string) =>
    `${project} ist eine Vorlage — Markierung entfernen`,
  projectsTemplateEmptyTitle: "Noch keine Vorlagen",
  projectsTemplateChooseProject: "Projekt wählen",
  projectsTemplateEmptyBody:
    "Öffnen Sie ein Projekt, das Sie genauso wieder durchführen würden, und drücken Sie den Stern daneben. Es bleibt ein gewöhnliches Board — es kann nur zusätzlich kopiert werden.",
  projectsTemplateFailed: "Das hat nicht geklappt.",
  projectsTemplatesLoadFailed: "Die Vorlagen konnten nicht geladen werden.",

  // Wo eine Woche steht. Das Wort des Servers, nie im Browser neu hergeleitet.
  projectsWeekOpen: "Offen",
  projectsWeekSubmitted: "Eingereicht",
  projectsWeekApproved: "Genehmigt",
  projectsWeekRejected: "Zurückgewiesen",

  // Der Genehmigungs-Eingang — der eine Bildschirm hier, der Personen nennt.
  projectsPerson: "Person",
  projectsSubmittedAt: "Eingereicht",
  projectsApprove: "Genehmigen",
  projectsApprovalComplete: "Woche genehmigt",
  projectsApprovalCompleteBody:
    "Sehen Sie sich die betroffenen Projekte an und rechnen Sie Kundenarbeit ab, die bereit ist.",
  projectsReject: "Zurückweisen",
  projectsRejectTitle: "Diese Woche zurückweisen?",
  projectsRejectBody: (person: string) =>
    `${person} wird lesen, was Sie hier schreiben.`,
  projectsRejectPlaceholder: "Was zu korrigieren ist",
  projectsApprovalsEmptyTitle: "Nichts zu genehmigen",
  projectsApprovalsEmptyBody:
    "Wochen, die eingereicht werden, landen hier — die älteste zuerst.",

  // Der Rentabilitätsbericht — Stunden × Sätze gegen ein Budget. Die Texte
  // sagen „Wert“ und nie „Marge“: das hier ist die Erlösseite.
  projectsReportTitle: "Rentabilität",
  projectsReportPortfolioTitle: "Portfoliobericht",
  projectsReportAllScope: "Alle Kundenprojekte, auf die Sie Zugriff haben.",
  projectsReportFrom: "Von",
  projectsReportTo: "Bis",
  projectsReportShow: "Anzeigen",
  projectsReportThisQuarter: "Dieses Quartal",
  projectsReportLastQuarter: "Letztes Quartal",
  projectsReportDownloadCsv: "CSV herunterladen",
  projectsReportDownloadFailed:
    "Der Bericht konnte nicht heruntergeladen werden.",
  projectsReportBasis: (from: string, to: string) =>
    `Gearbeitete Stunden zwischen ${from} und ${to}.`,
  projectsReportBudgetBasis: (to: string) =>
    `Budgets zählen alles bis ${to}, nicht nur diesen Zeitraum.`,
  projectsReportColValue: "Wert",
  projectsReportColInvoiced: "Abgerechnet",
  projectsReportColToInvoice: "Abzurechnen",
  projectsReportColToDate: "Stunden bisher",
  projectsReportColBudget: "Budget verbraucht",
  projectsReportTotals: "Alle Projekte zusammen",
  projectsReportUnrated: (duration: string) => `${duration} nicht bewertet`,
  projectsReportUnratedHint:
    "Abrechenbare Stunden ohne Satz. Sie werden hier gezählt und nirgends bewertet — hinterlegen Sie einen Stundensatz und erfassen Sie sie dann.",
  projectsReportNoValue: "Noch kein Wert",
  projectsReportBudgetLeft: (amount: string) => `${amount} übrig`,
  projectsReportBudgetOver: (amount: string) => `${amount} darüber`,
  projectsReportNoBudget: "Kein Budget festgelegt",
  projectsReportEmptyTitle: "Noch keine Kundenprojekte",
  projectsReportEmptyBody:
    "Rentabilität sind Stunden gegen einen Satz und ein Budget — sie beginnt also mit einem Kundenprojekt. Geben Sie einem Projekt einen Kunden und einen Satz, und diese Seite füllt sich.",

  // Das laufende Timer-Widget in der Leiste.
  projectsTimerRunning: "Timer läuft",
  projectsStopTimer: "Timer anhalten",
  projectsStop: "Stopp",

  // Die Projektwerkzeuge des Agenten (tranche 6). Erfasste Zeit ist ein
  // Vorschlag, bis die Person, deren Stundenzettel es ist, ihn annimmt; die
  // Statuszusammenfassung liest nur.
  agentActLogTime: "Zeit erfassen",
  agentActProjectStatus: "Projektstatus",
  agentFieldProject: "Projekt",
  agentFieldDay: "Tag",
  agentFieldDuration: "Dauer",
  agentLogTimeNote:
    "Schlägt einen Eintrag in Ihrem Stundenzettel vor — er zählt erst, wenn Sie ihn dort annehmen.",
  agentProjectStatusNote: "Liest das Projekt nur — nichts wird geändert.",
  agentTimeLogged: (project: string): string =>
    `In Ihrem Stundenzettel für ${project} vorgeschlagen — er zählt erst, wenn Sie ihn unter Projekte annehmen.`,
  agentStatusHours: "Erfasste Stunden",
  agentStatusBillable: (formatted: string): string =>
    `${formatted} abrechenbar`,
  agentStatusBudget: "Budget",
  agentStatusBudgetUsed: (percent: string): string => `${percent} verbraucht`,
  agentStatusNoBudget: "Kein Stundenbudget festgelegt",
  agentStatusInternal: "Internes Projekt — kein Kunde, kein Budget.",
  agentStatusCustomer: "Kunde",
  agentStatusMilestones: "Meilensteine",
  agentStatusMilestonesDone: (done: number, total: number): string =>
    `${done} von ${total} erreicht`,
  agentStatusMilestonesLate: (late: number): string =>
    late === 1 ? "1 überfällig" : `${late} überfällig`,
  agentStatusNoMilestones: "Keine geplant",
  agentStatusNext: "Als Nächstes",
  agentStatusTasks: "Aufgaben",
  agentStatusTasksOpen: (open: number): string =>
    open === 1 ? "1 offen" : `${open} offen`,
  agentStatusTasksOverdue: (overdue: number): string => `${overdue} überfällig`,
  agentStatusLastWorked: "Zuletzt gearbeitet",
  agentStatusNeverWorked: "Noch keine Stunden",
  // Der Kalender-Entwurf: ein Stapel Vorschläge, plus das, was er ausließ —
  // der Server schickt Gründe als Codes, jedes Wort dafür steht hier.
  agentActDraftTimesheet: "Stundenzettel aus Ihrem Kalender",
  agentDraftTimesheetNote:
    "Schlägt je Termin in Ihrem Kalender an diesen Tagen einen Eintrag vor — jeder zählt erst, wenn Sie ihn unter Projekte annehmen.",
  agentDraftedCount: (count: number): string =>
    count === 1 ? "1 Eintrag vorgeschlagen" : `${count} Einträge vorgeschlagen`,
  agentDraftedNone: "Nichts vorzuschlagen",
  agentDraftedRange: (from: string, to: string): string =>
    from === to ? from : `${from} – ${to}`,
  agentDraftedTotal: "Gesamt",
  agentDraftedOverlap: "überschneidet sich mit dem vorherigen",
  agentDraftedOverlaps: (count: number): string =>
    count === 1
      ? "1 davon überschneidet sich mit einem anderen Termin — prüfen Sie, welcher die Arbeit war."
      : `${count} davon überschneiden sich mit anderen Terminen — prüfen Sie, welche die Arbeit waren.`,
  agentDraftedNote: (project: string): string =>
    `In Ihrem Stundenzettel für ${project} vorgeschlagen — nehmen Sie jeden unter Projekte an, damit er zählt.`,
  agentDraftedLeftOut: "Ausgelassen",
  agentDraftedReason: (reason: string): string => {
    switch (reason) {
      case "allDay":
        return "ganztägig — keine Arbeitsstunden";
      case "alreadyDrafted":
        return "steht schon in Ihrem Stundenzettel";
      case "noDuration":
        return "keine Dauer";
      case "tooLong":
        return "länger als ein Tag";
      case "weekLocked":
        return "diese Woche ist eingereicht";
      case "limitReached":
        return "über dem Stapellimit — fragen Sie für die übrigen Tage erneut";
      case "outsideRange":
        return "beginnt außerhalb dieser Tage";
      default:
        // Ein Grund, den ein neuerer Server kennt: lieber „ausgelassen“
        // sagen, als so zu tun, als wäre etwas entworfen worden.
        return "ausgelassen";
    }
  },
  // Das Kategorisieren-Werkzeug des Finanz-Agenten. Ein Vorschlag ist keine
  // Zuordnung, und jedes Wort auf der Karte sagt das.
  agentActCategorise: "Kategorien vorschlagen",
  agentCategoriseNote:
    "Sieht Ihre eigenen Ausgaben ohne Kategorie durch und schlägt für jede eine vor — aus den Kategorien, die Sie für diesen Händler schon verwendet haben. Zugeordnet wird erst, wenn Sie annehmen.",
  agentCategoriseFieldPeriod: "Ausgaben aus",
  agentCategoriseSuggested: (count: number): string =>
    count === 1 ? "1 Vorschlag" : `${count} Vorschläge`,
  agentCategoriseNone: "Nichts vorzuschlagen",
  agentCategoriseConsidered: (count: number): string =>
    count === 1 ? "1 Ausgabe angesehen" : `${count} Ausgaben angesehen`,
  agentCategoriseEvidence: (times: number): string =>
    times === 1
      ? "schon einmal hier gebucht"
      : `schon ${times}-mal hier gebucht`,
  agentCategoriseAccept: "Annehmen",
  agentCategoriseDecline: "Nein",
  agentCategoriseAccepted: "Angenommen",
  agentCategoriseDeclined: "Abgelehnt",
  agentCategoriseLeftOut: "Ausgelassen",
  agentCategoriseNoMerchant: "Kein Händler",
  agentCategoriseFooter:
    "Jeder Vorschlag wartet auf Sie — gebucht, berichtet oder gemeldet wird erst, wenn Sie ihn annehmen.",
  agentCategoriseFailed:
    "Das konnte nicht beantwortet werden — versuchen Sie es unter Finanzen erneut.",
  agentCategoriseReason: (reason: string): string => {
    switch (reason) {
      case "noMerchant":
        return "kein Händler, an dem sie zu erkennen wäre";
      case "noHistory":
        return "Sie haben diesem Händler noch nie eine Kategorie gegeben";
      case "alreadyProposed":
        return "hat schon einen Vorschlag";
      case "declined":
        return "Sie haben hier einen Vorschlag abgelehnt";
      default:
        // Ein Grund, den ein neuerer Server kennt: lieber „ausgelassen“
        // sagen, als so zu tun, als wäre etwas vorgeschlagen worden.
        return "ausgelassen";
    }
  },
  // Die zwei Antworten des Finanz-Agenten. Beide lesen nur — und die Karte
  // sagt mehr als einmal, dass nichts abgegeben wurde.
  agentActVatSummary: "MwSt.-Zahlen",
  agentVatSummaryNote:
    "Liest die MwSt., die Ihre Bücher für diese Tage enthalten — berechnete Steuer, gezahlte Steuer und die Differenz. Nichts wird abgegeben und nichts wird geändert.",
  agentVatFieldPeriod: "Zeitraum",
  agentVatCharged: "Auf Verkäufe berechnet",
  agentVatPaid: "Auf Einkäufe gezahlt",
  agentVatOwed: "Sie schulden",
  agentVatRefund: "Sie bekommen zurück",
  agentVatBaseSales: "Umsatz",
  agentVatBaseCosts: "Kosten",
  agentVatUnrated: "Ohne Satz",
  agentVatRateRow: (rate: string, base: string): string =>
    `${rate} von ${base}`,
  agentVatNothing: "Nichts in diesen Tagen",
  agentVatFooter:
    "Zahlen für eine Erklärung, keine Erklärung — abgegeben wird weiterhin im Portal Ihres Landes.",
  // Die Bücher-Prüfung. Jedes Wort hier ist eine Frage, nie ein Urteil.
  agentActFlagAnomalies: "Bücher prüfen",
  agentAnomalyNote:
    "Liest Ihr Journal für diese Tage und nennt, was einen zweiten Blick wert ist — mit den Buchungen dahinter. Es schreibt nichts und markiert nichts als geprüft.",
  agentAnomalyFieldPeriod: "Bücher aus",
  agentAnomalyFound: (count: number): string =>
    count === 1 ? "1 einen Blick wert" : `${count} einen Blick wert`,
  agentAnomalyNone: "Nichts ist aufgefallen",
  agentAnomalyScanned: (count: number): string =>
    count === 1 ? "1 Buchung gelesen" : `${count} Buchungen gelesen`,
  agentAnomalyShown: (shown: number, found: number): string =>
    `${shown} von ${found} gezeigt`,
  agentAnomalyTruncated:
    "Diese Tage enthalten mehr Buchungen, als eine Prüfung liest — fragen Sie für einen kürzeren Zeitraum erneut, um den Rest zu sehen.",
  agentAnomalyNotComparable: (count: number): string =>
    count === 1
      ? "1 Buchung nennt weder Kunde noch Lieferant und konnte deshalb nicht verglichen werden"
      : `${count} Buchungen nennen weder Kunde noch Lieferant und konnten deshalb nicht verglichen werden`,
  agentAnomalyKind: (kind: string): string => {
    switch (kind) {
      case "duplicate":
        return "Zweimal in einer Woche gebucht";
      case "unusualAmount":
        return "Anders als der Rest dieses Kontos";
      case "missingRecurring":
        return "Ein Monat ohne Buchung";
      default:
        // Eine Art, die ein neuerer Server kennt: immer noch eine Frage,
        // nie nichts.
        return "Einen Blick wert";
    }
  },
  agentAnomalyTypical: (amount: string): string => `sonst ${amount}`,
  agentAnomalyMissingMonth: (month: string): string => `nichts im ${month}`,
  agentAnomalyEvidence: "Die Buchungen dahinter",
  agentAnomalyFooter:
    "Nichts wurde geändert und nichts als geprüft markiert — jedes davon ist eine Frage zu Buchungen, und die Antwort auf eine ist eine Korrekturbuchung.",

  // alo Finanzen (tranche 6). Die Ausgaben-Wörter sind die einer Person über
  // ihr eigenes Geld — „zurückgezahlt“, nicht „Erstattung verarbeitet“; die
  // Bank-Wörter sind die einer Buchhaltung — eine Datei ist ein
  // „Kontoauszug“, die Zahlungsreferenz heißt „Verwendungszweck“, wie auf
  // jeder deutschen Banking-Oberfläche. Kein Text nennt eine Regel, die der
  // Server besitzt: seine Ablehnung erscheint in seinen eigenen Worten.
  financeTabExpenses: "Ausgaben",
  financeTabApprovals: "Genehmigungen",
  financeClaimsTable: "Ihre Ausgaben",
  financeClaimFilters: "Ausgabenfilter",
  financeChartFilters: "Diagrammzeitraum",
  financeStatementsTable: "Importierte Kontoauszüge",
  financeChartTableOf: (kind: string) => `Konten — ${kind}`,
  financePendingClaimsTable: "Zu entscheidende Ausgaben",
  financeOwedClaimsTable: "Zurückzuzahlende Ausgaben",
  financeBankSampleTable: "Beispieltransaktionen",
  financeBankSettledTable: "Zugeordnete Bankzeilen",
  financeBankSetAsideTable: "Beiseitegelegte Bankzeilen",
  financeBankFilters: "Auszugsfilter",
  financeReportPeriod: "Berichtszeitraum",
  financeLoadFailed: "Ihre Ausgaben konnten nicht geladen werden.",
  financeSaveFailed: "Die Änderung konnte nicht gespeichert werden.",
  financeCancel: "Abbrechen",
  financeSave: "Speichern",
  financeEdit: "Bearbeiten",
  financeDelete: "Löschen",
  financeActions: "Aktionen",
  financeShow: "Anzeigen",
  financeFrom: "Von",
  financeTo: "Bis",

  // Die Ausgabe selbst.
  financeNewClaim: "Neue Ausgabe",
  financeEditClaim: "Ausgabe bearbeiten",
  financeClaimSubtitle: "Was Sie ausgegeben haben und wessen Geld bezahlt hat.",
  financeSpentOn: "Datum",
  financeSpentOnHint:
    "Der Tag, an dem das Geld abging — in Ihrer eigenen Zeitzone.",
  financeMerchant: "Händler",
  financeMerchantHint: "Wer bezahlt wurde — der Name auf dem Beleg.",
  financeNoMerchant: "Kein Händler",
  financeClaimOf: (merchant: string, day: string) => `${merchant}, ${day}`,
  financeDescription: "Wofür es war",
  financeGross: "Gesamtbetrag",
  financeVat: "MwSt.",
  financeVatHint:
    "Die auf dem Beleg ausgewiesene MwSt. Leer lassen, wenn keine ausgewiesen ist.",
  financeNoVat: "—",
  financeVatRate: "MwSt.-Satz %",
  financeVatRateHint: "Wie aufgedruckt: 19, 21, 5,5.",
  financeCurrency: "Währung",
  financeCurrencyHint: "Leer lassen für die Währung Ihres Arbeitsbereichs.",
  financeProject: "Projekt",
  financeProjectHint:
    "Ordnen Sie die Ausgabe Kundenarbeit zu, damit sie in den Kosten dieses Projekts erscheint.",
  financeNoProject: "Kein Projekt",
  financeMethod: "Bezahlt mit",
  financeMethodHint: "Nur eigenes Geld wird am Ende zurückgezahlt.",
  financeMethodPersonal: "Eigenes Geld",
  financeMethodCard: "Firmenkarte",
  financeMethodCash: "Handkasse",
  financeMethodPersonalOption: "Mein eigenes Geld",
  financeMethodCardOption: "Die Firmenkarte",
  financeMethodCashOption: "Die Handkasse",
  financeAmountInvalid: "Das ist kein Betrag.",
  financeRateInvalid: "Das ist kein Prozentsatz.",

  // Wo eine Ausgabe steht. Das Wort des Servers, in der Sprache der Person.
  financeStatus: "Status",
  financeAnyStatus: "Jeder Status",
  financeStatusDraft: "Entwurf",
  financeStatusSubmitted: "Wartet",
  financeStatusApproved: "Genehmigt",
  financeStatusRejected: "Abgelehnt",
  financeStatusReimbursed: "Zurückgezahlt",
  financePaidBackOn: (day: string) => `Zurückgezahlt am ${day}`,

  // Die Verben.
  financeSubmit: "Einreichen",
  financeWithdraw: "Zurücknehmen",
  financeApprove: "Genehmigen",
  financeReject: "Ablehnen",
  financeMarkPaidBack: "Als zurückgezahlt markieren",
  financeMarkPaidBackSubtitle: (person: string, amount: string) =>
    `${amount} zurück an ${person}.`,
  financeReimbursedOn: "Zurückgezahlt am",
  financeReimbursedOnHint:
    "Der Tag, an dem das Geld tatsächlich floss — auf diesen Tag wird gebucht.",
  financeDeleteTitle: "Diese Ausgabe löschen?",
  financeDeleteBody:
    "Die Ausgabe und alles, was Sie eingetragen haben, werden entfernt. Das lässt sich nicht rückgängig machen.",
  financeRejectTitle: "Diese Ausgabe ablehnen",
  financeRejectBody: (person: string) =>
    `${person} wird das lesen und kann die Ausgabe korrigieren und erneut einreichen.`,
  financeRejectPlaceholder: "Warum sie zurückkommt…",

  // Der Bildschirm der genehmigenden Person.
  financePerson: "Person",
  financeCategory: "Kategorie",
  financeUncategorised: "Ohne Kategorie",
  financeSubmittedAt: "Eingereicht",
  financeApprovedAt: "Genehmigt",
  financeOfWhichVat: (amount: string) => `inkl. ${amount} MwSt.`,
  financeWaitingTitle: "Wartet auf eine Entscheidung",
  financeWaitingEmptyTitle: "Nichts wartet",
  financeWaitingEmptyBody:
    "Ausgaben, die Ihre Kolleginnen und Kollegen einreichen, erscheinen hier — der älteste Kauf zuerst.",
  financeOwedTitle: "Zurückzuzahlen",
  financeOwedNote:
    "Genehmigte Ausgaben, die Ihre Kolleginnen und Kollegen aus eigener Tasche bezahlt haben. Eine Ausgabe, die die Firmenkarte bezahlt hat, ist genehmigt und schuldet niemandem etwas — deshalb steht sie nicht hier.",
  financeOwedEmptyTitle: "Niemandem wird etwas geschuldet",
  financeOwedEmptyBody:
    "Sobald Sie eine Ausgabe genehmigen, die jemand selbst bezahlt hat, wartet sie hier, bis das Geld zurückgeht.",

  // Das Erste, was Mitarbeitende vom Modul sehen.
  financeExpensesEmptyTitle: "Keine Ausgaben in diesem Zeitraum",
  financeExpensesEmptyBody:
    "Erfassen Sie, was Sie für die Arbeit ausgegeben haben — das Datum, den Gesamtbetrag auf dem Beleg und wessen Geld bezahlt hat. Sie bleibt bei Ihnen, bis Sie sie einreichen.",

  // Die Bank und der Stapel, den sie hinterlässt.
  financeTabBank: "Bank",
  financeTabReconcile: "Abgleich",
  financeBankLoadFailed: "Die Kontoauszüge konnten nicht geladen werden.",

  // Einen Auszug importieren.
  financeBankImportStatement: "Kontoauszug importieren",
  financeBankImportTitle: "Einen Kontoauszug importieren",
  financeBankImportSubtitle:
    "Wir lesen die Datei zuerst und zeigen Ihnen, was wir daraus gemacht haben. Gespeichert wird erst, wenn Sie es sagen.",
  financeBankFile: "Auszugsdatei",
  financeBankFileHint:
    "Ein CAMT.053- oder MT940-Download Ihrer Bank oder ein CSV-Export.",
  financeBankAccount: "Konto",
  financeBankAccountHint:
    "Die IBAN, zu der dieser Auszug gehört. Eine CAMT.053- oder MT940-Datei nennt sie selbst; eine CSV nicht.",
  financeBankCurrencyHint:
    "Für eine CSV, die es nicht sagt. Leer lassen für die Währung Ihres Arbeitsbereichs.",
  financeBankCheckFile: "Datei prüfen",
  financeBankCheckAgain: "Erneut prüfen",
  financeBankImport: "Importieren",
  financeBankReadFailed: "Diese Datei konnte nicht gelesen werden.",
  financeBankImportFailed: "Es wurde nichts importiert.",
  financeBankStale:
    "Sie haben geändert, wie die Datei gelesen wird. Prüfen Sie sie erneut, um das Ergebnis zu sehen.",
  financeBankStaged: (staged: number, duplicates: number) =>
    duplicates === 0
      ? `${staged} Transaktionen importiert.`
      : `${staged} Transaktionen importiert; ${duplicates} waren schon da und wurden nicht angetastet.`,

  // Was der Server aus der Datei gemacht hat.
  financeBankFormat: "Gelesen als",
  financeBankSourceCamt: "CAMT.053",
  financeBankSourceMt940: "MT940",
  financeBankSourceCsv: "CSV",
  financeBankRows: "Transaktionen",
  financeBankRowsRead: (lines: number, rows: number) =>
    `${lines} von ${rows} Zeilen`,
  financeBankSkipped: "Zeilen, die keine Transaktionen sind",
  financeBankUnbooked: "Von der Bank noch nicht gebucht",
  financeBankPeriod: "Zeitraum",
  financeBankEncoding: "Zeichenkodierung",
  financeBankSampleTitle:
    "Die ersten Transaktionen, so wie wir sie gelesen haben",
  financeBankSampleTruncated:
    "Hier werden nur die ersten Transaktionen gezeigt. Importiert werden alle.",
  financeBankRowsRefused: (count: number) =>
    count === 1
      ? "Eine Zeile kann nicht gelesen werden, deshalb wurde nichts importiert."
      : `${count} Zeilen können nicht gelesen werden, deshalb wurde nichts importiert.`,
  financeBankRowAt: (line: number) => `Zeile ${line}:`,
  financeBankRowUnknown: "Eine Zeile:",

  // Uns sagen, welche Spalte was ist.
  financeBankMappingTitle: "Welche Spalte was ist",
  financeBankMappingNote:
    "Wir haben es aus der Kopfzeile der Datei geraten. Korrigieren Sie, was wir falsch haben, und prüfen Sie die Datei dann erneut.",
  financeBankColumnNone: "Nicht in dieser Datei",
  financeBankColDate: "Buchungsdatum",
  financeBankColValueDate: "Wertstellung",
  financeBankColAmount: "Betrag (eine Spalte mit Vorzeichen)",
  financeBankColDebit: "Geldausgang",
  financeBankColCredit: "Geldeingang",
  financeBankColSign: "In welche Richtung es geht",
  financeBankColCurrency: "Währung je Zeile",
  financeBankColCounterparty: "Wer bezahlt wurde oder wer bezahlt hat",
  financeBankColIban: "Das Konto der Gegenseite",
  financeBankColRemittance: "Verwendungszweck",
  financeBankColReference: "Die eigene Referenz der Bank",
  financeBankDates: "Datumsangaben gelesen als",
  financeBankDecimal: "Cent getrennt durch",
  financeBankConventionAuto: "Aus der Datei ableiten",
  financeBankConventionDmy: "Tag/Monat/Jahr",
  financeBankConventionMdy: "Monat/Tag/Jahr",
  financeBankConventionYmd: "Jahr-Monat-Tag",
  financeBankConventionComma: "Ein Komma",
  financeBankConventionDot: "Ein Punkt",

  // Was importiert wurde.
  financeBankLines: "Transaktionen",
  financeBankClosingBalance: "Endsaldo",
  financeBankImportedAt: "Importiert",
  financeBankEmptyTitle: "Noch keine Kontoauszüge",
  financeBankEmptyBody:
    "Importieren Sie einen Monat von Ihrer Bank, und jede Transaktion darin landet auf einem Stapel und wartet darauf, den Rechnungen zugeordnet zu werden, die sie bezahlt hat.",

  // Der Abgleich-Bildschirm.
  financeBankStatement: "Kontoauszug",
  financeBankAllStatements: "Alles noch nicht Zugeordnete",
  financeBankToMatchTitle: (count: number) =>
    count === 1
      ? "1 Transaktion zuzuordnen"
      : `${count} Transaktionen zuzuordnen`,
  financeBankAllMatchedTitle: "Nichts mehr zuzuordnen",
  financeBankAllMatchedBody:
    "Jede Transaktion in den importierten Auszügen ist entweder einer Rechnung zugeordnet oder beiseitegelegt. Importieren Sie einen weiteren Monat, um weiterzumachen.",
  financeBankCapped:
    "Diese Liste ist ein erster Stapel, nicht alles — arbeiten Sie sie durch und laden Sie neu, um den Rest zu sehen.",
  financeBankBookedOn: "Gebucht",
  financeBankCounterparty: "Wer",
  financeBankNoCounterparty: "Kein Name auf der Zahlung",
  financeBankRemittance: "Verwendungszweck",
  financeBankCertain: "Sicher",
  financeBankThisOne: "Diese hier",
  financeBankNoGuess:
    "Wir haben keine Ahnung, was das hier ist. Wählen Sie die Rechnung, oder legen Sie es beiseite.",
  financeBankNotOurs: "Nicht unsere",
  financeBankPickInvoice: "Rechnung auswählen",
  financeBankStillOwed: "noch offen",
  financeBankStillOwedIs: (amount: string) => `${amount} noch offen`,
  financeBankMatchFailed: "Diese Transaktion wurde nicht zugeordnet.",
  financeBankUnmatchFailed: "Diese Zuordnung wurde nicht zurückgenommen.",
  financeBankIgnoreFailed: "Diese Transaktion wurde nicht beiseitegelegt.",

  // Warum wir glauben, dass eine Transaktion ein Dokument beglichen hat.
  financeBankWhyNumberQuoted:
    "unsere Rechnungsnummer steht im Verwendungszweck",
  financeBankWhyRuleSaved: "dieser Zahler wurde schon einmal so zugeordnet",
  financeBankWhyCustomerNamed: (percent: number) =>
    `der Name auf der Zahlung ähnelt dem des Kunden (${percent} %)`,
  financeBankWhyWholeAmount: "der Betrag entspricht genau dem, was offen ist",
  financeBankWhyOnlyDocument:
    "es ist die einzige offene Rechnung über diesen Betrag",
  financeBankWhyBeforeDue: (days: number) =>
    days === 1
      ? "sie kam einen Tag vor Fälligkeit an"
      : `sie kam ${days} Tage vor Fälligkeit an`,
  financeBankWhyAfterDue: (days: number) =>
    days === 1
      ? "sie kam einen Tag nach Fälligkeit an"
      : `sie kam ${days} Tage nach Fälligkeit an`,
  financeBankWhyPartPayment: (amount: string) =>
    `sie ist eine Teilzahlung der Rechnung — ${amount} wären noch offen`,

  // Eine Transaktion beiseitelegen.
  financeBankIgnoreTitle: "Nicht von uns zu buchen",
  financeBankIgnoreBody:
    "Sagen Sie warum, damit die nächste Person, die diesen Auszug liest, es nicht noch einmal herausfinden muss. Bankgebühren, eine private Überweisung, ein Duplikat.",
  financeBankIgnore: "Beiseitelegen",
  financeBankIgnorePlaceholder: "Warum sie nicht unsere ist…",

  // Die Rechnung von Hand auswählen.
  financeBankPickTitle: "Welche Rechnung hat das beglichen?",
  financeBankPickSubtitle: (amount: string) =>
    `Eingegangen: ${amount}. Sagen Sie, was damit bezahlt wurde.`,
  financeBankFindInvoice: "Rechnung suchen",
  financeBankFindInvoiceHint:
    "Nach Nummer oder nach der Referenz, die Ihr Kunde ihr gegeben hat.",
  financeBankNoOpenInvoices:
    "Keine ausgestellte Rechnung wartet noch auf Geld.",
  financeBankNoNumber: "Ohne Nummer",
  financeBankOverdue: "Überfällig",
  financeBankConfirmMatch: "Diese hat sie beglichen",

  // Was schon erledigt ist.
  financeBankUnmatched: "Zuzuordnen",
  financeBankMatched: "Zugeordnet",
  financeBankIgnored: "Beiseitegelegt",
  financeBankSettledTitle: "Schon zugeordnet",
  financeBankSettledNote:
    "Jede davon hat eine Zahlung erfasst und die Bücher bewegt. Eine Zurücknahme kehrt das mit einer eigenen Buchung um.",
  financeBankUndoMatch: "Zurücknehmen",
  financeBankSetAsideTitle: "Beiseitegelegt",
  financeBankSetAsideNote:
    "Transaktionen, von denen jemand entschieden hat, dass sie nicht von uns zu buchen sind.",
  financeBankUndoIgnore: "Zurück auf den Stapel",

  // Der Kontenplan. Eine Aufgabe wird als der Satz angeboten, den sie
  // bedeutet („Was Kunden uns schulden“), nie als das Wort der Leitung; die
  // Buchungsregeln folgen der Aufgabe, nie der Nummer, und ein Konto mit
  // Verlauf wird stillgelegt, nie gelöscht.
  financeTabAccounts: "Konten",
  financeChartLoadFailed: "Der Kontenplan konnte nicht geladen werden.",
  financeChartSeeded:
    "Wir haben Ihnen einen neutralen Kontenplan angelegt. Jedes dieser Konten dürfen Sie umbenennen oder umnummerieren — die Nummerierung Ihrer Steuerberatung macht nichts kaputt, denn die Buchhaltung folgt der Aufgabe eines Kontos und nicht seiner Nummer.",
  financeChartEmptyTitle: "Noch keine Konten",
  financeChartEmptyBody:
    "Der Kontenplan ist die Liste der Orte, an denen Geld sein kann: die Bank, was Kunden Ihnen schulden, was Sie verdienen, was Sie ausgeben. Gebucht werden kann erst, wenn es einen gibt.",

  financeAccountAdd: "Konto hinzufügen",
  financeAccountEdit: "Bearbeiten",
  financeAccountDelete: "Löschen",
  financeAccountCode: "Nummer",
  financeAccountCodeHint:
    "So nennt es Ihre Steuerberatung. Buchstaben und Ziffern, keine Leerzeichen.",
  financeAccountName: "Name",
  financeAccountRole: "Aufgabe",
  financeAccountRoleHint:
    "Wofür dieses Konto automatisch verwendet wird. Rechnungen, Zahlungen und Ausgaben finden ihr Konto über seine Aufgabe, nie über seine Nummer — Umnummerieren ist deshalb gefahrlos, und wird eine Aufgabe entzogen, buchen diese Belege nicht mehr, bis ein anderes Konto sie übernimmt.",
  financeAccountType: "Art",
  financeAccountTypeHint:
    "Was das Konto enthält. Sie entscheidet, in welchem Bericht das Konto erscheint.",
  financeAccountTypeUnset: "Bitte wählen…",
  financeAccountActive: "In Gebrauch",
  financeAccountActiveHint:
    "Ein stillgelegtes Konto behält seinen Verlauf und seinen Saldo und wird auf neuen Belegen nicht mehr angeboten.",
  financeAccountInUse: "In Gebrauch",
  financeAccountRetired: "Stillgelegt",
  financeAccountShowRetired: "Stillgelegte anzeigen",
  financeAccountMovement: "Bewegung",
  financeAccountPostings: "Buchungen",
  financeAccountSystemNote:
    "Dieses Konto haben wir angelegt, deshalb kann es nicht gelöscht werden — die Buchhaltung läuft darüber. Benennen Sie es um, nummerieren Sie es um, oder legen Sie es still.",
  financeAccountNewTitle: "Konto hinzufügen",
  financeAccountNewBody: "Ihre eigene Zeile in Ihrem eigenen Kontenplan.",
  financeAccountEditTitle: "Konto bearbeiten",
  financeAccountEditBody:
    "Umbenennen und Umnummerieren sind jederzeit gefahrlos.",
  financeAccountSaveFailed: "Das Konto wurde nicht gespeichert.",
  financeAccountDeleteFailed: "Das Konto wurde nicht gelöscht.",

  // Die fünf Arten, zweimal: das kurze Wort für die Tabelle und der Satz,
  // den jemand beim Auswählen tatsächlich beantwortet.
  financeAccountTypeAsset: "Was wir besitzen",
  financeAccountTypeLiability: "Was wir schulden",
  financeAccountTypeEquity: "Eigenkapital",
  financeAccountTypeIncome: "Was wir verdienen",
  financeAccountTypeExpense: "Was wir ausgeben",
  financeAccountTypeAssetLong:
    "Etwas, das wir besitzen oder das uns geschuldet wird — ein Bankkonto, Bargeld, Forderungen an Kunden",
  financeAccountTypeLiabilityLong:
    "Etwas, das wir schulden — Lieferanten, Steuern, Geld für Mitarbeitende",
  financeAccountTypeEquityLong:
    "Der Anteil der Eigentümer und die Salden, mit denen die Bücher eröffnet wurden",
  financeAccountTypeIncomeLong: "Etwas, das wir verdienen",
  financeAccountTypeExpenseLong: "Etwas, das wir ausgeben",

  // Die Aufgaben, über die eine Buchungsregel aufgelöst wird.
  financeRoleNone: "Keine besondere Aufgabe",
  financeRoleAr: "Was Kunden uns schulden",
  financeRoleAp: "Was wir Lieferanten schulden",
  financeRoleBank: "Das Bankkonto, über das Geld läuft",
  financeRoleCash: "Handkasse",
  financeRoleVatOutput: "MwSt., die wir berechnet haben und schulden",
  financeRoleVatInput:
    "MwSt., die wir gezahlt haben und uns erstatten lassen können",
  financeRoleRevenue: "Umsatzerlöse",
  financeRoleExpenseDefault: "Kosten ohne eigene Kategorie",
  financeRoleEmployeePayable: "Ausgaben, die wir Mitarbeitenden schulden",
  financeRoleFxDiff: "Kursdifferenzen",
  financeRoleRounding: "Rundungsdifferenzen",
  financeRoleOpeningBalance: "Die Salden, mit denen die Bücher eröffnet wurden",
  financeRoleSuspense: "Geld, das wir noch nicht zuordnen können",

  // Die vier Berichte. Jede Zahl ist die Faltung des Journals durch den
  // Server, in ganzen Cent; die Wörter sind die einer Unternehmerin, wo sie
  // es sein können („Was wir besitzen“), und die einer Buchhalterin, wo sie
  // es sein müssen („Eigenkapital“).
  financeTabReports: "Berichte",
  financeReportPl: "Gewinn- und Verlustrechnung",
  financeReportBalance: "Bilanz",
  financeReportAged: "Wer was schuldet",
  financeReportVat: "MwSt.-Erklärung",
  financeReportFrom: "Von",
  financeReportTo: "Bis",
  financeReportOn: "Zum",
  financeReportShow: "Anzeigen",
  financeReportToday: "Heute",
  financeReportThisYear: "Dieses Jahr",
  financeReportThisQuarter: "Dieses Quartal",
  financeReportLastQuarter: "Letztes Quartal",
  financeReportLastYearEnd: "Ende letzten Jahres",
  financeReportDownloadCsv: "CSV herunterladen",
  financeReportDownloadFailed: "Die Datei konnte nicht heruntergeladen werden.",
  financeReportLoadFailed: "Der Bericht konnte nicht geladen werden.",
  financeReportBasis: (from: string, to: string) =>
    `Alles, was zwischen ${from} und ${to} gebucht wurde, beide Tage eingeschlossen.`,
  financeReportBasisOn: (on: string) =>
    `Alles, was bis einschließlich ${on} gebucht wurde.`,
  financeReportEmptyTitle: "Noch nichts gebucht",
  financeReportEmptyBody:
    "Ausgestellte Rechnungen, Zahlungen und genehmigte Ausgaben buchen sich von selbst. Sobald eine das tut, erscheint sie hier.",
  financeReportAmount: "Betrag",
  financeReportTotal: "Gesamt",
  financeReportPrevious: (from: string, to: string) => `${from} – ${to}`,

  // Die Gewinn- und Verlustrechnung.
  financeReportIncome: "Was wir verdient haben",
  financeReportIncomeTotal: "Verdient insgesamt",
  financeReportExpense: "Was wir ausgegeben haben",
  financeReportExpenseTotal: "Ausgegeben insgesamt",
  financeReportProfit: "Gewinn",
  financeReportLoss: "Verlust",

  // Die Bilanz.
  financeReportAssets: "Was wir besitzen",
  financeReportAssetsTotal: "Besitz insgesamt",
  financeReportLiabilities: "Was wir schulden",
  financeReportLiabilitiesTotal: "Geschuldet insgesamt",
  financeReportEquity: "Eigenkapital",
  financeReportEquityTotal: "Eigenkapital insgesamt",
  financeReportResultToDate:
    "Gewinn oder Verlust bisher, noch nicht ins Eigenkapital abgeschlossen",
  financeReportLiabilitiesEquityTotal:
    "Schulden, Eigenkapital und Ergebnis zusammen",
  financeReportDifference: "Differenz",
  financeReportUnbalanced: (amount: string) =>
    `Diese Bücher gehen nicht auf: ein Betrag von ${amount} ist unerklärt. Reichen Sie nichts von diesem Blatt ein — schicken Sie es stattdessen an uns.`,

  // Wer was schuldet.
  financeReportSide: "Ansicht",
  financeReportReceivable: "Was uns geschuldet wird",
  financeReportPayable: "Was wir schulden",
  financeReportParty: "Wer",
  financeReportBandCurrent: "Noch nicht fällig",
  financeReportBand1To30: "1–30 Tage",
  financeReportBand31To60: "31–60 Tage",
  financeReportBand61To90: "61–90 Tage",
  financeReportBand90Plus: "Über 90 Tage",
  financeReportOpenDocuments: (count: number) =>
    count === 1 ? "1 offenes Dokument" : `${count} offene Dokumente`,
  financeReportNothingOwedToUs: "Niemand schuldet Ihnen etwas",
  financeReportNothingWeOwe: "Sie schulden niemandem etwas",
  financeReportAgedEmptyBody:
    "Jedes ausgestellte Dokument auf dieser Seite ist vollständig beglichen.",
  financeReportUnconverted: (count: number) =>
    count === 1
      ? "1 Dokument steht in keiner dieser Spalten: Wir haben keinen Wechselkurs, um es in Ihrer eigenen Währung anzugeben."
      : `${count} Dokumente stehen in keiner dieser Spalten: Wir haben keinen Wechselkurs, um sie in Ihrer eigenen Währung anzugeben.`,

  // Die MwSt.-Erklärung.
  financeReportVatRate: "Satz",
  financeReportVatBase: "Betrag vor MwSt.",
  financeReportVatTax: "MwSt.",
  financeReportVatOutput: "MwSt., die wir berechnet haben",
  financeReportVatOutputTotal: "Berechnet insgesamt",
  financeReportVatInput: "MwSt., die wir gezahlt haben",
  financeReportVatInputTotal: "Gezahlt insgesamt",
  financeReportVatUnrated: "Ohne angegebenen Satz",
  financeReportVatPayable: "Zu zahlen",
  financeReportVatRefund: "Zu erstatten",
  financeReportVatNote:
    "Das sind die Zahlen Ihrer Bücher — Verkäufe und Einkäufe zusammen —, und aus ihnen wird eine Erklärung abgegeben. Die MwSt.-Übersicht unter Rechnungen zeigt, was Sie in Rechnung gestellt haben — eine andere Frage.",

  // ---- Tranche 7: der Assistent überall, Base, Lager, Personen und
  // Kampagnen -----------------------------------------------------------------
  //
  // Der Rest des Agent-Schwanzes: die Karten, die der Assistent in E-Mail,
  // Kalender, Chat und Drive zeigt. Die Verben sind dieselben wie auf den
  // Bildschirmen, deren Arbeit sie auslösen (Kennzeichnen, Zurückstellen,
  // In Ordner verschieben) — zwei Wörter für einen Knopf wären zwei Knöpfe.
  agentActWhatsOn: "Ihren Kalender lesen",
  agentActAmIFree: "Auf Überschneidung prüfen",
  agentActCatchUp: "Nachlesen, was gesagt wurde",
  agentActFindInChat: "Unterhaltungen durchsuchen",
  agentActFindFile: "Ihr Drive durchsuchen",
  agentActFindContact: "Kontakt nachschlagen",
  agentFieldRoom: "Unterhaltung",
  agentFieldLookingFor: "Gesucht",
  agentActDraft: "Neue E-Mail",
  agentActReply: "Antworten",
  agentActSend: "E-Mail senden",
  agentActArchive: "Archivieren",
  agentActTrash: "In den Papierkorb",
  agentActMarkRead: "Als gelesen markieren",
  agentActMarkUnread: "Als ungelesen markieren",
  agentActFlag: "Kennzeichnen",
  agentActUnflag: "Kennzeichnung entfernen",
  agentActSnooze: "Zurückstellen",
  agentActMove: "In Ordner verschieben",
  agentActTask: "Aufgabe erstellen",
  agentActEvent: "In den Kalender eintragen",
  agentSendButton: "Senden",
  agentSendCaution:
    "Damit wird die E-Mail jetzt gesendet — das lässt sich nicht rückgängig machen.",
  agentFieldTo: "An",
  agentFieldSubject: "Betreff",
  agentFieldEmail: "E-Mail",
  agentFieldReplyTo: "Als Antwort auf",
  agentFieldUntil: "Bis",
  agentFieldFolder: "Ordner",
  agentFieldDue: "Fällig",
  agentFieldWhen: "Wann",
  agentFieldTask: "Aufgabe",
  agentFieldEvent: "Termin",
  agentNoSubject: "(kein Betreff)",
  // Die Lager-Werkzeuge des Assistenten: Entwürfe und eine Auskunft, nie eine
  // Bestellung.
  agentActReorderProposals: "Nachbestellungen entwerfen",
  agentReorderNote:
    "Sieht alles durch, was unter Ihrem eigenen Mindestbestand liegt, und schreibt je Lieferant einen Bestellentwurf. Nichts wird gesendet — jeder Entwurf wartet bei Ihren Bestellungen darauf, dass Sie ihn prüfen und senden.",
  agentActStockAnswer: "Bestand prüfen",
  agentStockAnswerNote:
    "Liest, wo ein Produkt gerade steht: im Regal, bestellt, Kunden zugesagt. Es ändert nichts und reserviert nichts.",
  agentFieldSupplier: "Lieferant",
  agentFieldLocation: "Ort",
  agentFieldProduct: "Produkt",
  agentReorderEverySupplier: "Jeder Lieferant",
  agentReorderEverywhere: "Überall",
  agentReorderShortages: (count: number): string =>
    count === 1 ? "1 unter Minimum" : `${count} unter Minimum`,
  agentReorderNothingShort: "Nichts liegt unter seinem Minimum",
  agentReorderDrafted: (count: number): string =>
    count === 1 ? "1 Bestellentwurf" : `${count} Bestellentwürfe`,
  agentReorderLines: (count: number): string =>
    count === 1 ? "1 Position" : `${count} Positionen`,
  agentReorderLeftOut: "Nichts bestellt für",
  agentReorderReason: (reason: string): string => {
    switch (reason) {
      case "noSupplier":
        return "niemand hat Ihnen dafür einen Preis genannt";
      case "nothingToBuy":
        return "die Regel verlangt nichts";
      default:
        // Ein Grund, den ein neuerer Server kennt und dieser Client nicht:
        // sichtbar ausgelassen, nie stillschweigend verschluckt.
        return "ausgelassen";
    }
  },
  agentReorderNeeded: (qty: string, unit: string): string =>
    unit === "" ? `${qty} benötigt` : `${qty} ${unit} benötigt`,
  agentReorderFooter:
    "Das sind Entwürfe. Kein Lieferant wurde kontaktiert und keine Bestellnummer gezogen — öffnen Sie einen im Lager, um ihn zu prüfen und zu senden.",
  agentStockOnHand: "Im Regal",
  agentStockOnOrder: "Bestellt",
  agentStockCommitted: "Zugesagt",
  agentStockAvailable: "Bleibt",
  agentStockNoShelf: "Eine Dienstleistung — nichts liegt auf Lager",
  agentStockNowhere: "Nirgends etwas",
  agentStockWatched: "Gehalten bei",
  agentStockMinimum: (min: string, target: string): string =>
    `Minimum ${min}, aufgefüllt auf ${target}`,
  agentStockBelowMinimum: "unter Minimum",
  agentStockFooter:
    "Zahlen, wie sie jetzt gerade stehen. Nichts wurde bestellt und nichts zurückgelegt.",
  // Die eine HR-Auskunft: Namen und Tage, nie ein Grund.
  agentActWhoIsOff: "Sehen, wer abwesend ist",
  agentWhoIsOffNote:
    "Liest die Abwesenheitsübersicht, die hier ohnehin alle sehen: wer abwesend ist, und an welchen Tagen. Es ändert nichts, trägt nichts ein und benachrichtigt niemanden.",
  agentWhoIsOffAway: "Abwesend",
  agentWhoIsOffNobody: "Niemand",
  agentWhoIsOffCount: (count: number): string =>
    count === 1 ? "1 Person" : `${count} Personen`,
  agentWhoIsOffDays: (count: number): string =>
    count === 1 ? "1 Tag" : `${count} Tage`,
  agentWhoIsOffFooter:
    "Nur Namen und Tage — eine genehmigte Abwesenheit sagt nie, warum jemand fehlt. Wer nicht aufgeführt ist, kann trotzdem aus einem Grund fehlen, den diese Übersicht nicht abdeckt.",

  // Base im Drive. Der Typname Base bleibt unübersetzt wie in jeder Sprache;
  // Einträge heißen Einträge, nicht Datensätze — es ist ein Werkzeug für
  // alle, kein Datenbankverwaltungssystem.
  baseNewRow: "Neue Zeile",
  baseAddField: "Feld hinzufügen",
  baseFieldName: "Feldname",
  baseNewTable: "Neue Tabelle",
  baseTypeText: "Text",
  baseTypeNumber: "Zahl",
  baseTypeDate: "Datum",
  baseTypeCheckbox: "Checkbox",
  baseTypeSelect: "Auswahl",
  baseTypeMultiselect: "Mehrfachauswahl",
  baseTypePerson: "Person",
  baseTypeLink: "Verknüpfung zu Tabelle",
  baseViewGrid: "Raster",
  baseViewBoard: "Board",
  baseViewCalendar: "Kalender",
  baseViewGallery: "Galerie",
  baseAddView: "Ansicht hinzufügen",
  baseGroupBy: "Gruppieren nach…",
  baseByDate: "Nach Datum…",
  baseChoicesPlaceholder: "Optionen, durch Kommas getrennt",
  baseLinkTarget: "Verknüpfte Tabelle…",
  baseUncategorised: "Ohne Kategorie",
  baseBoardNeedsSelect:
    "Fügen Sie eine Board-Ansicht hinzu, die nach einem Auswahlfeld gruppiert, um dies zu nutzen.",
  baseCalendarNeedsDate:
    "Fügen Sie eine Kalenderansicht auf Basis eines Datumsfelds hinzu, um dies zu nutzen.",
  baseBoardEmptyTitle: "Einträge auf einem Board gruppieren",
  baseCalendarEmptyTitle: "Einträge auf einen Kalender legen",
  baseBoardEmptyBody:
    "Boards gruppieren Einträge nach einem Auswahlfeld. Fügen Sie ein fertiges Statusfeld hinzu, um fortzufahren.",
  baseCalendarEmptyBody:
    "Kalender ordnen Einträge nach einem Datumsfeld an. Fügen Sie eines hinzu, um fortzufahren.",
  baseAddStatusField: "Statusfeld hinzufügen",
  baseAddDateField: "Datumsfeld hinzufügen",
  baseStatusField: "Status",
  baseDateField: "Datum",
  baseStatusTodo: "Zu erledigen",
  baseStatusInProgress: "In Arbeit",
  baseStatusDone: "Erledigt",
  baseCalendarPreviousMonth: "Vormonat",
  baseCalendarNextMonth: "Folgemonat",
  baseCalendarAddOnDate: (date: string): string =>
    `Eintrag am ${date} hinzufügen`,
  baseLoading: "Ihre Base wird geladen…",
  baseLoadFailedTitle: "Diese Base wurde nicht geladen",
  baseEmptyTitle: "Beginnen Sie mit Ihrer ersten Tabelle",
  baseEmptyBody:
    "Tabellen halten zusammengehörige Einträge beisammen. Legen Sie eine an, um Felder und Einträge hinzuzufügen.",
  baseDefaultTableName: (number: number): string => `Tabelle ${number}`,
  baseView: "Ansicht",
  baseSaveChanges: "Änderungen speichern",
  baseUntitledRecord: "Ohne Titel",
  basePersonPlaceholder: "email@…",
  baseNoChoices: "Noch keine Optionen — ergänzen Sie welche am Feld.",
  baseLink: "Verknüpfen",
  baseLinkNoTable: "Keine verknüpfte Tabelle festgelegt.",
  baseLinkNoRecords: "Die verknüpfte Tabelle hat noch keine Einträge.",

  // alo Lager. Die deutsche Handelssprache trennt, was das Englische teilt:
  // eine Bestellung geht an den Lieferanten, ein Auftrag kommt vom Kunden,
  // und wo beide auf einer Spalte stehen, heißt das Dokument Beleg. Die
  // Bewegungsgründe tragen die Wörter des Lagers selbst (Wareneingang,
  // Umlagerung, Inventur, Schwund), nicht Übersetzungen unserer englischen.
  inventoryTabCatalog: "Katalog",
  inventoryTabStock: "Bestand",
  inventoryLoadFailed: "Ihr Katalog konnte nicht geladen werden.",
  inventorySaveFailed: "Die Änderung konnte nicht gespeichert werden.",
  inventoryHistoryFailed: "Dieser Verlauf konnte nicht geladen werden.",
  inventoryClose: "Schließen",
  inventoryEdit: "Bearbeiten",
  inventoryArchive: "Archivieren",
  inventoryRestore: "Wiederherstellen",
  inventoryArchived: "archiviert",
  inventoryColActions: "Aktionen",
  inventoryNoMatches: "Nichts hier passt zu Ihrer Eingabe.",
  inventoryNewProduct: "Neues Produkt",
  inventorySearchCatalog: "Nach Name, Code oder Barcode suchen",
  inventoryStockedOnly: "Nur Lagerware",
  inventoryShowArchived: "Archivierte anzeigen",
  inventoryCatalogEmptyTitle: "Ihr Katalog ist leer",
  inventoryCatalogEmptyBody:
    "Ein Produkt ist hier ein Eintrag: was Sie dafür berechnen, was Sie dafür zahlen und — wenn es etwas ist, das Sie im Regal führen — wie viel Sie davon haben. Legen Sie das erste an, und es kann noch am selben Tag auf eine Rechnung und in ein Lager.",
  inventoryColProduct: "Produkt",
  inventoryColSku: "Code",
  inventoryColBarcode: "Barcode",
  inventoryColOnHand: "Bestand",
  inventoryColPurchasePrice: "Wir zahlen",
  inventoryColSalePrice: "Wir berechnen",
  inventoryColVatRate: "MwSt.",
  inventoryTypeStocked: "Lagerware",
  inventoryTypeService: "Dienstleistung",
  inventoryNotStocked: "—",
  inventoryArchiveProductConfirm: (name: string) =>
    `${name} archivieren? Es bleibt auf jedem bereits erstellten Dokument stehen und wird auf neuen nicht mehr angeboten. Sie können es jederzeit wiederherstellen.`,
  inventoryFieldSku: "Code (SKU)",
  inventorySkuHint:
    "Ihr eigener Code für diesen Artikel. Eindeutig unter Ihren Produkten; lassen Sie ihn leer, wenn Sie keinen haben.",
  inventoryFieldBarcode: "Barcode",
  inventoryBarcodeHint:
    "Die GTIN auf der Verpackung. Ihre Prüfziffer wird geprüft — ein vertippter Code wird hier abgewiesen, statt erst aufzufallen, wenn das Falsche versendet wird.",
  inventoryFieldPurchasePrice: "Einkaufspreis",
  inventoryPurchasePriceHint: "Was Sie dafür zahlen, in Ihrer eigenen Währung.",
  inventoryFieldDefaultSupplier: "Üblicher Lieferant",
  inventoryDefaultSupplierHint:
    "Bei wem das normalerweise gekauft wird. Davon geht ein Nachbestellvorschlag aus.",
  inventoryNoSupplier: "Niemand Bestimmtes",
  inventoryFieldStocked: "Bestand",
  inventoryStockedLabel: "Eine Menge davon führen",
  inventoryStockedHint:
    "Nur Lagerware kann zwischen Orten bewegt werden. Eine Dienstleistung lässt sich weder annehmen noch liefern noch zählen — und sobald etwas bewegt wurde, lässt sich dies nicht mehr abschalten.",
  inventorySearchStock: "Nach Produkt, Code oder Ort suchen",
  inventoryFilterLocation: "Ort",
  inventoryAllLocations: "Überall",
  inventoryShowCounterparties: "Gegenseiten anzeigen",
  inventoryCounterpartiesNote:
    "Lieferanten, Kunden, Korrekturen und Produktion sind Gegenseiten, keine Orte: Sie sind das andere Ende jeder Bewegung. Werden sie angezeigt, summiert sich die Gesamtzahl unten auf ungefähr nichts — so sieht ein Bestandsbuch aus, das aufgeht, nicht ein leeres Lager.",
  inventoryStockEmptyTitle: "Noch liegt nichts im Regal",
  inventoryStockEmptyBody:
    "Bestand erscheint hier, wenn sich etwas bewegt: eine Bestellung, die Sie erhalten, eine Lieferung, die Sie senden, oder eine Korrektur von Hand. Es gibt keine Menge zum Eintippen — was hier steht, ist die Summe von allem, was geschehen ist.",
  inventoryColLocation: "Ort",
  inventoryColValue: "Wert",
  inventoryColLastMove: "Letzte Bewegung",
  inventoryOpenHistory: "Verlauf",
  inventoryReferenceValue: (total: string) =>
    `${total} zu heutigen Einkaufspreisen — ein Richtwert für das Aufgeführte, keine Buchhaltungszahl.`,
  inventoryHistoryTitle: (product: string) => `${product} — Bewegungen`,
  inventoryHistorySubtitle: (place: string) =>
    `Alles, was bei ${place} ein- oder ausgegangen ist.`,
  inventoryHistoryEmpty: "An diesem Ort ist noch nichts ein- oder ausgegangen.",
  inventoryHistoryCapped: (limit: number) =>
    `Die letzten ${limit} Bewegungen werden angezeigt. Ältere bleiben aufgezeichnet.`,
  inventoryColWhen: "Wann",
  inventoryColMovement: "Von → nach",
  inventoryColQuantity: "Menge",
  inventoryColWhy: "Warum",
  inventoryColDocument: "Dokument",
  inventoryNoDocument: "Von Hand",
  inventoryKindStock: "Lager",
  inventoryKindTransit: "Unterwegs",
  inventoryKindSupplier: "Lieferant",
  inventoryKindCustomer: "Kunde",
  inventoryKindAdjust: "Korrektur",
  inventoryKindProduction: "Produktion",
  inventoryReasonReceipt: "Wareneingang",
  inventoryReasonDelivery: "Warenausgang",
  inventoryReasonTransfer: "Umlagerung",
  inventoryReasonAdjustment: "Bestandskorrektur",
  inventoryReasonReturn: "Retoure",
  inventoryReasonShrinkage: "Schwund",
  inventoryReasonCount: "Inventur",
  inventoryAdjustDamaged: "Bruch",
  inventoryAdjustLost: "Verloren",
  inventoryAdjustFound: "Gefunden",
  inventoryAdjustExpired: "Abgelaufen",
  inventoryAdjustTheft: "Diebstahl",
  inventoryAdjustSample: "Muster",
  inventoryAdjustCorrection: "Korrektur",
  inventoryTabPurchasing: "Einkauf",
  inventoryTabSales: "Aufträge",
  inventoryOrdersLoadFailed: "Diese Aufträge konnten nicht geladen werden.",
  inventoryOrderLoadFailed: "Dieser Auftrag konnte nicht geladen werden.",
  inventoryDraftOrder: "Entwurf",
  inventoryDraftInvoice: "Rechnungsentwurf",
  inventoryOrderLate: "Überfällig",
  inventoryFilterStatus: "Status",
  inventoryAllStatuses: "Jeder Status",
  inventoryNoOrdersInState: "Keine Aufträge in diesem Status",
  inventoryCancelAction: "Abbrechen",
  inventoryOrderStatusCancelled: "Storniert",
  inventoryPoStatusDraft: "Entwurf",
  inventoryPoStatusSent: "Bestellt",
  inventoryPoStatusPartial: "Teilweise erhalten",
  inventoryPoStatusReceived: "Erhalten",
  inventorySoStatusDraft: "Entwurf",
  inventorySoStatusConfirmed: "Bestätigt",
  inventorySoStatusPartial: "Teilweise geliefert",
  inventorySoStatusDelivered: "Geliefert",
  inventorySearchPurchaseOrders: "Nach Nummer, Lieferant oder Referenz suchen",
  inventorySearchSalesOrders: "Nach Nummer, Kunde oder Referenz suchen",
  inventoryNewPurchaseOrder: "Neue Bestellung",
  inventoryNewSalesOrder: "Neuer Auftrag",
  inventoryPurchaseOrdersEmptyTitle: "Sie haben noch nichts bestellt",
  inventoryPurchaseOrdersEmptyBody:
    "Eine Bestellung hält fest, was Sie bei einem Lieferanten angefragt haben. Legen Sie sie als Entwurf an, geben Sie sie auf, wenn Sie so weit sind, und buchen Sie ein, was ankommt — das Bestandsbuch wird für Sie geschrieben.",
  inventorySalesOrdersEmptyTitle: "Noch hat kein Kunde etwas bestellt",
  inventorySalesOrdersEmptyBody:
    "Ein Auftrag hält fest, was ein Kunde bei Ihnen bestellt hat. Legen Sie ihn als Entwurf an, bestätigen Sie ihn, damit er seine Nummer erhält, und buchen Sie jede Sendung beim Rausgehen — die Rechnung berechnet, was tatsächlich gegangen ist.",
  inventoryColOrder: "Beleg",
  inventoryColSupplier: "Lieferant",
  inventoryColCustomer: "Kunde",
  inventoryColExpected: "Erwartet",
  inventoryColPromised: "Zugesagt",
  inventoryColState: "Status",
  inventoryColTotal: "Summe",
  inventoryTabOrderBook: "Auftragsbuch",
  inventoryOrderBookLoadFailed: "Das Auftragsbuch konnte nicht geladen werden.",
  inventoryFilterScope: "Anzeigen",
  inventoryScopeOpen: "Offene Aufträge",
  inventoryScopeAll: "Alle Aufträge",
  inventoryColOrdered: "Bestellt",
  inventoryColReserved: "Reserviert",
  inventoryColInvoiced: "Berechnet",
  inventoryBookTotal: "Über alle zusammen",
  inventoryBookMixedCurrencies: (currencies: string) =>
    `Diese Aufträge lauten auf ${currencies}, deshalb gibt es keine einzelne Summe. Die Zahlen jedes einzelnen Auftrags sind exakt.`,
  inventoryBookQtyHint: (qtyMilli: string) => `Noch ausstehend: ${qtyMilli}`,
  inventoryOrderBookEmptyTitle: "Nichts steht aus",
  inventoryOrderBookEmptyBody:
    "Das Auftragsbuch zeigt, worauf Kunden warten und was Sie ihnen noch zu berechnen haben. Bestätigen Sie einen Auftrag, und er erscheint hier, bis das Letzte davon rausgegangen und berechnet ist.",
  inventoryOrderBookEmptyAllTitle: "Noch keine Aufträge angelegt",
  inventoryOrderBookEmptyAllBody:
    "Noch wurde nichts verkauft — nicht einmal ein Entwurf. Das Auftragsbuch füllt sich von selbst, sobald Aufträge angelegt werden.",
  inventoryBackToPurchaseOrders: "Alle Bestellungen",
  inventoryBackToSalesOrders: "Alle Aufträge",
  inventoryCreateDraft: "Entwurf anlegen",
  inventorySaveDraft: "Speichern",
  inventoryPrintOrder: "Drucken",
  inventoryUnsavedNotice:
    "Diese Änderungen sind noch nicht gespeichert; die Summen unten sind die letzten, die der Server berechnet hat.",
  inventoryOrderFrozenNotice:
    "Diese Bestellung ist aufgegeben. Sie trägt eine Nummer, die der Lieferant kennt, und lässt sich deshalb nicht mehr bearbeiten — buchen Sie ein, was ankommt, oder stornieren Sie sie.",
  inventorySalesOrderFrozenNotice:
    "Dieser Auftrag ist bestätigt. Er trägt eine Nummer, die der Kunde kennt, und lässt sich deshalb nicht mehr bearbeiten — buchen Sie jede Sendung beim Rausgehen.",
  inventoryFixLinesFirst:
    "Eine der Positionen ist nicht fertig. Beheben Sie das und speichern Sie erneut.",
  inventoryOrderNeedsSupplier:
    "Wählen Sie den Lieferanten, bei dem diese Bestellung aufgegeben wird.",
  inventoryOrderNeedsCustomer:
    "Wählen Sie den Kunden, für den dieser Auftrag ist.",
  inventoryPickSupplier: "Lieferant wählen",
  inventoryPickCustomer: "Kunden wählen",
  inventorySupplierHint:
    "Bei wem Sie bestellen. Nach dem Aufgeben der Bestellung lässt sich das nicht mehr ändern.",
  inventoryCustomerHint:
    "Für wen der Auftrag ist. Nach dem Bestätigen lässt sich das nicht mehr ändern.",
  inventoryExpectedHint:
    "Der Tag, an dem Sie die Ware erwarten. Eine Bestellung darüber hinaus gilt als überfällig.",
  inventoryPromisedHint:
    "Der Tag, zu dem Sie die Ware zugesagt haben. Ein Auftrag darüber hinaus gilt als überfällig.",
  inventoryFieldReference: "Referenz",
  inventoryReferenceHint:
    "Ihre eigene Referenz für diesen Vorgang — ein Projekt, eine Baustelle, eine Auftragsnummer.",
  inventoryFieldOrdered: "Aufgegeben",
  inventoryFieldConfirmed: "Bestätigt",
  inventoryFieldNote: "Notiz",
  inventoryOrderNoteHint:
    "Alles, was die Gegenseite lesen soll. Es wird auf den Beleg gedruckt.",
  inventoryLines: "Positionen",
  inventoryAddLine: "Position hinzufügen",
  inventoryNoLines: "Noch keine Positionen.",
  inventoryColDescription: "Beschreibung",
  inventoryColUnit: "Einheit",
  inventoryColUnitPrice: "Einzelpreis",
  inventoryColNet: "Netto",
  inventoryColReceived: "Erhalten",
  inventoryColDelivered: "Geliefert",
  inventoryColOutstanding: "Ausstehend",
  inventoryColToBill: "Zu berechnen",
  inventoryPickProduct: "Aus dem Katalog",
  inventoryDescriptionPlaceholder: "Was bestellt wird",
  inventoryUnitPlaceholder: "Stück",
  inventoryQtyPlaceholder: "1",
  inventoryAmountPlaceholder: "0,00",
  inventoryRatePlaceholder: "0",
  inventoryRemoveLine: "Position entfernen",
  inventoryLineNeedsDescription: "Sagen Sie, wofür diese Position ist.",
  inventoryNotAQuantity: "Das ist keine Menge.",
  inventoryNotAnAmount: "Das ist kein Betrag.",
  inventoryNotARate: "Das ist kein Satz.",
  inventorySendOrder: "Bestellung aufgeben",
  inventorySendOrderConfirm:
    "Damit erhält die Bestellung ihre Nummer, wird endgültig eingefroren, und das Begleitschreiben mit der gedruckten Bestellung im Anhang wird in Ihre Entwürfe gelegt. Gesendet wird nichts, bis Sie es selbst senden.",
  inventoryOrderPlacedNotice: (to: string, file: string) =>
    `Die Bestellung ist aufgegeben. Ein Begleitschreiben an ${to} mit ${file} im Anhang wartet in Ihren Entwürfen — gesendet wurde nichts.`,
  inventoryConfirmOrder: "Auftrag bestätigen",
  inventoryConfirmOrderConfirm:
    "Damit erhält der Auftrag seine Nummer und wird endgültig eingefroren. Eine Nachricht schreibt das nicht: Den Kunden zu informieren ist ein gewöhnlicher Brief, den Sie selbst senden.",
  inventoryCancelOrder: "Stornieren",
  inventoryCancelOrderConfirm:
    "Der Beleg bleibt erhalten und lesbar, aber es wird nichts mehr darauf erwartet.",
  inventoryCancelShortConfirm:
    "Ein Teil davon wurde bereits bewegt. Mit dem Stornieren gilt das bisher Abgewickelte als das Ganze, und mehr wird nicht erwartet. Der Beleg bleibt lesbar.",
  inventoryDiscardDraft: "Entwurf verwerfen",
  inventoryDiscardDraftConfirm:
    "Dieser Entwurf hat keine Nummer und wurde niemandem gezeigt — er wird deshalb gelöscht, nicht storniert.",
  inventoryReceiveGoods: "Eingang buchen",
  inventoryDeliverGoods: "Sendung buchen",
  inventoryReceiveTitle: (order: string) => `Was zu ${order} angekommen ist`,
  inventoryDeliverTitle: (order: string) => `Was zu ${order} rausgeht`,
  inventoryReceiveSubtitle:
    "Jede Position beginnt mit dem noch Ausstehenden. Ändern Sie, was fehlt; der Rest bleibt bestellt. Für das Angekommene wird eine Eingangsrechnung als Entwurf angelegt.",
  inventoryDeliverSubtitle:
    "Jede Position beginnt mit dem noch Ausstehenden. Ändern Sie, was jetzt geht; der Rest bleibt auf dem Auftrag.",
  inventoryReceiveWhere: "Eingelagert in",
  inventoryReceiveWhereHint:
    "Wohin die Ware tatsächlich gelegt wurde. Das Bestandsbuch wird für diesen Ort geschrieben.",
  inventoryDeliverWhere: "Entnommen aus",
  inventoryDeliverWhereHint:
    "Woraus die Ware entnommen wurde. Das Bestandsbuch wird für diesen Ort geschrieben.",
  inventoryColThisConsignment: "Diesmal",
  inventoryFulfilNoteHint:
    "Was die abwickelnde Person notiert hat — eine beschädigte Kiste, eine Teillieferung.",
  inventoryFulfilNeedsPlace: "Wählen Sie zuerst den Ort.",
  inventoryFulfilNeedsSomething:
    "Auf keiner Position steht etwas, also gibt es nichts zu buchen.",
  inventoryNoPlaces: "Noch keine Orte",
  inventoryBookArrival: "Einbuchen",
  inventoryBookConsignment: "Ausbuchen",
  inventoryArrivalBooked:
    "Der Eingang ist gebucht, das Bestandsbuch geschrieben, und eine Eingangsrechnung wartet als Entwurf auf Freigabe.",
  inventoryConsignmentBooked:
    "Die Sendung ist gebucht und das Bestandsbuch geschrieben.",
  inventoryArrivals: "Eingänge",
  inventoryNoArrivals: "Zu dieser Bestellung ist noch nichts angekommen.",
  inventoryArrivalNo: (n: number) => `Eingang ${n}`,
  inventoryBillDrafted: "Rechnungsentwurf angelegt",
  inventoryConsignments: "Sendungen",
  inventoryNoConsignments: "Zu diesem Auftrag ist noch nichts rausgegangen.",
  inventoryConsignmentNo: (n: number) => `Sendung ${n}`,
  inventoryRaiseInvoice: "Berechnen, was gegangen ist",
  inventoryRaisedInvoices: "Rechnungen",
  inventoryNoRaisedInvoices: "Aus diesem Auftrag wurde noch nichts berechnet.",
  inventoryInvoiceDrafted:
    "Für das Rausgegangene wurde ein Rechnungsentwurf angelegt. Er trägt keine Nummer, bis ihn jemand in Rechnungen ausstellt.",
  inventoryScan: "Scannen",
  inventoryScanTitle: "Barcode scannen",
  inventoryScanSubtitle:
    "Scannen Sie mit einem Handscanner in das Feld, oder tippen Sie den Code ein. Am Telefon geht stattdessen die Kamera.",
  inventoryScanFieldCode: "Barcode",
  inventoryScanPlaceholder: "4006381333931",
  inventoryScanHint:
    "Ein Handscanner tippt den Code hier ein und drückt für Sie die Eingabetaste. Leerzeichen und Bindestriche werden ignoriert.",
  inventoryScanLookup: "Nachschlagen",
  inventoryScanFailed: "Dieser Code konnte nicht nachgeschlagen werden.",
  inventoryScanWaiting: "Warten auf einen Code.",
  inventoryScanCameraStart: "Kamera verwenden",
  inventoryScanCameraStop: "Kamera stoppen",
  inventoryScanCameraFailed:
    "Die Kamera ließ sich nicht starten. Erlauben Sie den Zugriff, oder tippen Sie den Code ein — ein Handscanner braucht gar keine Berechtigung.",
  inventoryScanAiming:
    "Richten Sie die Kamera auf den Barcode. Sie stoppt, sobald sie einen gelesen hat.",
  inventoryScanNoCamera:
    "Dieser Browser kann keinen Barcode über die Kamera lesen. Ein Handscanner funktioniert hier: Er tippt in das Feld oben.",
  inventoryScanOnHand: (quantity: string) =>
    `${quantity} auf Lager, über alle Orte.`,
  inventoryScanNowhere: "Davon liegt noch nirgends etwas.",
  inventoryScanServiceNote:
    "Das ist eine Dienstleistung — eine Menge davon gibt es nicht zu finden.",
  inventoryScanOpenProduct: "Dieses Produkt öffnen",
  inventoryScanShowInStock: "In der Liste zeigen",
  inventoryScanAddProduct: "Mit diesem Barcode in den Katalog aufnehmen",

  // alo Personen. Die Vertragsarten tragen die Namen der deutschen Papiere
  // selbst (unbefristet, befristet, Ausbildung), eine Bewerbung, die nicht
  // weiterkommt, ist „nicht berücksichtigt" — das Wort des Absagebriefs,
  // nie eines gegen die Person —, und Ausgeschieden wird schlicht gesagt.
  // Die Abwesenheitsschicht kennt keinen Grund, also nennt kein Wort hier
  // einen: keine Krankheit, keine Elternzeit — Namen und Tage, sonst nichts.
  hrTabHiring: "Recruiting",
  hrTabTemplates: "Briefvorlagen",
  hrTemplatesTitle: "Briefvorlagen",
  hrTemplatesIntro:
    "Schreiben Sie freigegebene Formulierungen einmal auf; HR erstellt daraus einen persönlichen Entwurf, ohne neu zu tippen.",
  hrTemplatesLoadFailed: "Die Briefvorlagen konnten nicht geladen werden.",
  hrTemplatesEmpty: "Noch keine Briefvorlagen",
  hrTemplatesEmptyBody:
    "Legen Sie die Formulierungen an, die Ihr Unternehmen zu versenden bereit ist. Von diesem Bildschirm wird nichts gesendet.",
  hrTemplateNew: "Neue Vorlage",
  hrTemplateCreateTitle: "Briefvorlage anlegen",
  hrTemplateEditTitle: "Briefvorlage bearbeiten",
  hrTemplateEditorIntro:
    "Platzhalter werden erst gefüllt, wenn HR einen Entwurf für eine bestimmte Kollegin oder einen bestimmten Kollegen erstellt.",
  hrTemplateName: "Name der Vorlage",
  hrTemplateSubject: "Betreff der E-Mail",
  hrTemplateBody: "Wortlaut des Briefs",
  hrTemplateBodyHint:
    "Verwenden Sie die zugelassenen Platzhalter unten. Unbekannte Platzhalter werden abgewiesen.",
  hrTemplateInsertField: "Platzhalter einfügen",
  hrTemplateSave: "Vorlage speichern",
  hrTemplateSaveFailed: "Die Briefvorlage wurde nicht gespeichert.",
  hrTemplateDelete: "Vorlage löschen",
  hrTemplateDeleteTitle: (name: string) => `${name} löschen?`,
  hrTemplateDeleteBody:
    "Bestehende Briefentwürfe bleiben unverändert. Für neue Briefe steht diese Vorlage nicht mehr zur Verfügung.",
  hrTemplateDeleteFailed: "Die Briefvorlage wurde nicht gelöscht.",
  hrTemplateFields: (count: number) =>
    count === 1 ? "1 Platzhalter" : `${count} Platzhalter`,
  hrLoadFailed: "Das konnte nicht geladen werden.",
  hrSaveFailed: "Diese Änderung wurde nicht gespeichert.",
  hrClose: "Schließen",
  hrCancel: "Abbrechen",
  hrCreate: "Anlegen",
  hrSave: "Speichern",
  hrOpening: "Stelle",
  hrNewOpening: "Neue Stelle",
  hrEditOpening: "Stelle bearbeiten",
  hrOpeningSubtitle:
    "Eine aufgeschriebene Stelle. Veröffentlichen heißt: Die Runde läuft; Schließen beendet sie und friert ein, was die Stelle besagte.",
  hrPublishOpening: "Veröffentlichen",
  hrCloseOpening: "Runde schließen",
  hrCloseConfirm: (title: string) =>
    `Die Runde für ${title} schließen? Die Bewerbungen bleiben als Aufzeichnung dessen, was geschehen ist, und die Runde lässt sich nicht wieder öffnen.`,
  hrIncludeClosed: "Geschlossene Runden einbeziehen",
  hrClosedNotice:
    "Diese Runde ist geschlossen. Ihr Board lässt sich weiter lesen, und die Personen darauf lassen sich weiter verschieben — aber niemand Neues kann hinzukommen.",
  hrOpenedOn: (day: string) => `offen seit ${day}`,
  hrClosedOn: (day: string) => `geschlossen ${day}`,
  hrStatusDraft: "Entwurf",
  hrStatusOpen: "Offen",
  hrStatusClosed: "Geschlossen",
  hrFieldRole: "Stelle",
  hrFieldTeam: "Team",
  hrFieldLocation: "Ort",
  hrLocationHint: "Eine Stadt, ein Büro oder „remote“.",
  hrFieldEmployment: "Anstellung",
  hrKindPermanent: "Unbefristet",
  hrKindFixedTerm: "Befristet",
  hrKindPartTime: "Teilzeit",
  hrKindApprentice: "Ausbildung",
  hrKindContractor: "Selbstständig",
  hrKindIntern: "Praktikum",
  hrNoOpeningsTitle: "Noch keine Stelle aufgeschrieben",
  hrNoOpeningsBody:
    "Schreiben Sie die Stelle auf, die Sie besetzen wollen. Erfassen Sie die Bewerbungen, wie sie eintreffen, und schieben Sie die Personen über das Board, während Sie sie kennenlernen.",
  hrStage: "Phase",
  hrStageApplied: "Beworben",
  hrStageReviewing: "In Prüfung",
  hrStageInterview: "Gespräch",
  hrStageOffer: "Angebot",
  hrStageHired: "Eingestellt",
  hrStageRejected: "Nicht berücksichtigt",
  hrStageWithdrawn: "Zurückgezogen",
  hrCandidate: "Bewerbung",
  hrAddCandidate: "Bewerbung erfassen",
  hrEditCandidate: "Angaben bearbeiten",
  hrCandidateSubtitle:
    "Was in der Bewerbung stand. Nichts hier wird von einer Maschine gelesen — kein Screening, kein Ranking, keine Punktzahl.",
  hrFieldName: "Name",
  hrFieldEmail: "E-Mail",
  hrFieldPhone: "Telefon",
  hrFieldSource: "Woher die Bewerbung kam",
  hrSourceHint:
    "Ein Jobportal, eine Empfehlung, eine Agentur — wie auch immer die Bewerbung Sie erreicht hat.",
  hrAppliedOn: (moment: string) => `Beworben ${moment}`,
  hrNotes: "Gesprächsnotizen",
  hrNotesEmpty: "Noch nichts aufgeschrieben.",
  hrNotePlaceholder: "Was im Raum gesagt wurde…",
  hrAddNote: "Notiz hinzufügen",
  hrCv: "Lebenslauf",
  hrCvNone: "Kein Lebenslauf hinterlegt.",
  hrCvDownload: "Lebenslauf herunterladen",
  hrCvTrashed:
    "Der hinterlegte Lebenslauf wurde in den HR-Papierkorb verschoben.",
  hrCvFailed: "Diese Datei konnte nicht heruntergeladen werden.",
  hrCvAttach: "Lebenslauf anhängen",
  hrCvHint:
    "Abgelegt im HR-Bereich, wo nur HR ihn öffnen kann. Nichts liest ihn — kein Screening, kein Ranking, keine Punktzahl.",
  hrCvReplace: "Lebenslauf ersetzen",
  hrCvOnFile: (fileName: string) =>
    fileName === ""
      ? "Ein Lebenslauf ist hinterlegt. Eine neue Datei ersetzt ihn; der ersetzte wandert in den HR-Papierkorb."
      : `${fileName} ist hinterlegt. Eine neue Datei ersetzt ihn; der ersetzte wandert in den HR-Papierkorb.`,
  hrCvRemove: "Lebenslauf von dieser Bewerbung entfernen",
  hrCvUploadFailed:
    "Diese Datei wurde nicht hochgeladen, also wurde nichts gespeichert. Versuchen Sie es erneut, oder speichern Sie die Angaben ohne sie.",
  hrHired: "Die Stelle wurde angenommen",
  hrHiredExplainer:
    "Jemanden auf Eingestellt zu schieben hält fest, was geschehen ist. Die Person ins Verzeichnis zu schreiben ist ein eigener Schritt — er wird hier getan.",
  hrHire: "Ins Verzeichnis aufnehmen",
  hrHireSubmit: "Ins Verzeichnis aufnehmen",
  hrHireSubtitle:
    "Der Personalstammsatz und die Konditionen zum Start. Alles ist aus Bewerbung und Stelle vorausgefüllt — korrigieren Sie, was nicht stimmt.",
  hrHireKnown: (name: string) =>
    `${name} steht mit dieser Adresse bereits im Verzeichnis. Dieser Eintrag würde eine zweite Person mit derselben E-Mail anlegen.`,
  hrHireKnownLeft: (name: string) =>
    `${name} hatte diese Adresse und ist ausgeschieden. Kommt dieselbe Person zurück, ist ein neuer Eintrag hier richtig — der alte bleibt, wie er war.`,
  hrHireNameHint:
    "Aus dem Namen der Bewerbung aufgeteilt. Korrigieren Sie es, wenn die Teilung falsch war.",
  hrHireEmailHint:
    "Die dienstliche Adresse, falls schon bekannt. Sie kann später ergänzt werden.",
  hrHireStartHint:
    "Der Tag, an dem die Konditionen beginnen. Jedes Abwesenheitsguthaben wird von ihm an gezählt.",
  hrHireNoKind: "Nicht angegeben",
  hrHireNoAccount:
    "Das schreibt einen Eintrag in Personen. Es erstellt weder Anmeldung noch Postfach — das tut eine Administratorin oder ein Administrator, und die Onboarding-Checkliste hat eine Aufgabe dafür.",
  hrFieldGivenName: "Vorname",
  hrFieldFamilyName: "Nachname",
  hrFieldWorkEmail: "Dienstliche E-Mail",
  hrFieldJobTitle: "Position",
  hrFieldStartedOn: "Beginnt am",
  hrRetention: "Wie lange wir das aufbewahren",
  hrRetentionUntil: (day: string) => `Aufbewahrt bis ${day}.`,
  hrRetentionExpired: "Frist verstrichen",
  hrRetentionExplainer:
    "Nichts wird automatisch gelöscht. Ist das Datum verstrichen, entscheidet hier jemand — und was geht, geht: die Angaben, jede Notiz und der Lebenslauf.",
  hrFieldRetainUntil: "Aufbewahren bis",
  hrRetainHint:
    "Sechs Monate ab der Bewerbung, wenn Sie nichts anderes sagen. Nach diesem Datum kann der Eintrag gelöscht werden.",
  hrErase: "Diesen Eintrag löschen",
  hrEraseConfirm: (name: string) =>
    `Alles über ${name} löschen? Die Angaben, jede Notiz und der Lebenslauf werden endgültig entfernt. Das lässt sich nicht rückgängig machen.`,
  hrTabApprovals: "Freigaben",
  hrQueueLeave: "Abwesenheit",
  hrQueueExpense: "Ausgabe",
  hrQueueTimesheet: "Woche",
  hrPerson: "Person",
  hrWhat: "Wartet auf Sie",
  hrQueue: "Art",
  hrFigure: "Betrag",
  hrWaitingSince: "Eingereicht",
  hrActions: "Entscheidung",
  hrHiringControls: "Bewerbungsrunde",
  hrLeaveControls: "Abwesenheitsfilter",
  hrAwayControls: "Monat",
  hrDirectoryControls: "Verzeichnisfilter",
  hrLeaveTable: "Abwesenheitsanträge",
  hrApprovalsTable: "Wartet auf eine Entscheidung",
  hrDirectoryTable: "Personen",
  hrApprove: "Genehmigen",
  hrSendBack: "Zurückweisen",
  hrSendBackTitle: "Das zurückweisen?",
  hrSendBackBody: (person: string) =>
    `${person} sieht das wieder, bearbeitbar, mit dem, was Sie hier schreiben. Sagen Sie, was zu korrigieren ist.`,
  hrSendBackPlaceholder: "Was zu korrigieren ist",
  hrWaitingCount: (count: number) =>
    count === 1 ? "1 wartet" : `${count} warten`,
  hrCountOf: (kind: string, count: number) => `${kind}: ${count}`,
  hrWorkingDays: (days: number) => (days === 1 ? "1 Tag" : `${days} Tage`),
  hrLeaveOf: (policy: string, from: string, to: string) =>
    from === to ? `${policy}, ${from}` : `${policy}, ${from} – ${to}`,
  hrApprovalsEmptyTitle: "Nichts wartet",
  hrApprovalsEmptyBody:
    "Abwesenheiten, Ausgaben und Stundenzettel-Wochen, die eingereicht werden, landen hier gemeinsam, Ältestes zuerst — damit niemand wartet, nur weil der eigene Antrag im Modul lag, das Sie zuletzt geöffnet haben.",
  hrApprovalsNoneTitle: "Zu Ihnen kommt nichts zur Entscheidung",
  hrApprovalsNoneBody:
    "Hier warten Abwesenheiten, Ausgaben und Stundenzettel-Wochen auf die Person, die sie entscheidet. Sie sehen es, sobald jemand an Sie berichtet — oder wenn Sie die Bücher führen.",
  hrApprovalsQueueFailed: (kinds: string) =>
    `Ein Teil des Wartenden konnte nicht gelesen werden (${kinds}), diese Liste ist deshalb unvollständig. Alles Übrige wird angezeigt.`,
  hrApprovalsWidgetLabel: "wartend",
  hrApprovalsWidgetTitle:
    "Abwesenheiten, Ausgaben und Wochen, die auf Ihre Entscheidung warten",
  hrTabDirectory: "Verzeichnis",
  hrDirectorySearch: "Personen suchen",
  hrDirectoryViews: "Wie das Verzeichnis zu lesen ist",
  hrViewPeople: "Personen",
  hrViewOrg: "Organigramm",
  hrIncludeLeavers: "Ausgeschiedene einbeziehen",
  hrPeopleCount: (count: number) =>
    count === 1 ? "1 Person" : `${count} Personen`,
  hrShowingOf: (shown: number, total: number) => `${shown} von ${total}`,
  hrContact: "Kontakt",
  hrManager: "Berichtet an",
  hrSince: "Hier seit",
  hrYou: "Sie",
  hrLeft: "Ausgeschieden",
  hrShowInChart: "Im Organigramm zeigen",
  hrReportsCount: (count: number) =>
    count === 1 ? "1 unterstellte Person" : `${count} unterstellte Personen`,
  hrDirectoryEmptyTitle: "Noch niemand steht im Verzeichnis",
  hrDirectoryEmptyBody:
    "Sobald HR die erste Person aufschreibt, finden hier alle ihre Kolleginnen und Kollegen — wer sie sind, wie man sie erreicht und an wen sie berichten.",
  hrNoMatchTitle: (query: string) => `Niemand passt zu „${query}“`,
  hrNoMatchBody:
    "Gesucht wird in Namen, Positionen, Teams, E-Mail-Adressen und Telefonnummern, in beliebiger Reihenfolge. Versuchen Sie es mit einem Wort weniger.",
  hrClearSearch: "Suche leeren",
  hrTabLeave: "Meine Abwesenheiten",
  hrTabAway: "Wer ist abwesend",
  hrLeaveWhose: "Wessen Abwesenheit",
  hrScopeMine: "Meine",
  hrScopeTeam: "Mein Team",
  hrScopeEveryone: "Alle",
  hrLeaveShow: "Anzeigen",
  hrShowEverything: "Alles",
  hrShowWaiting: "Wartet auf Entscheidung",
  hrShowBooked: "Eingetragen",
  hrAskForLeave: "Abwesenheit beantragen",
  hrOneDay: "1 Tag",
  hrDaysOf: (days: string) => `${days} Tage`,
  hrFactOf: (label: string, value: string) => `${label} ${value}`,
  hrBalanceLeft: "übrig",
  hrBalanceThisYear: "Dieses Jahr",
  hrBalanceTaken: "Genommen",
  hrBalanceBooked: "Eingetragen",
  hrBalanceWaiting: "Ausstehend",
  hrBalanceAsOf: (day: string) =>
    `Berechnet am ${day}, nach Ihrem eigenen Arbeitszeitmodell.`,
  hrUnpaid: "Unbezahlt",
  hrNotDecided: "Erfasst, nicht entschieden",
  hrLeaveKind: "Art",
  hrLeaveWhen: "Wann",
  hrLeaveDays: "Tage",
  hrLeaveWhy: "Warum",
  hrLeaveState: "Status",
  hrLeaveBetween: (from: string, to: string) => `${from} – ${to}`,
  hrHolidaysInside:
    "In diese Tage fällt ein Feiertag; er wird nicht mitgezählt.",
  hrLeaveRequested: "Ausstehend",
  hrLeaveApproved: "Eingetragen",
  hrLeaveRejected: "Nicht genehmigt",
  hrLeaveWithdrawn: "Zurückgenommen",
  hrLeaveCancelled: "Storniert",
  hrWithdraw: "Zurücknehmen",
  hrCancelLeave: "Stornieren",
  hrLeaveEmptyTitle: "Sie haben noch keine Abwesenheit beantragt",
  hrLeaveEmptyBody:
    "Beantragen Sie hier einen Tag oder zwei Wochen. Sie sehen, was es Ihr Guthaben kostet, bevor jemand entscheidet — und wer an diesen Tagen schon abwesend ist.",
  hrLeaveTeamEmptyTitle: "Niemand hat Abwesenheit beantragt",
  hrLeaveTeamEmptyBody:
    "Wenn jemand, der an Sie berichtet, freie Tage beantragt, landet das hier und in Ihren Freigaben — mit den Daten, was es das Guthaben kostet, und wer dann sonst noch abwesend ist.",
  hrLeaveNoneShownTitle: "Nichts in diesem Status",
  hrLeaveNoneShownBody:
    "Es sind Abwesenheiten erfasst, aber keine davon im gewählten Status.",
  hrAskSubtitle:
    "Die Tage gehen vom Guthaben der gewählten Art ab, berechnet nach Ihrem eigenen Arbeitszeitmodell — eine Anzahl Tage tippen Sie nie.",
  hrAskSubmit: "Beantragen",
  hrPolicyRecordedHint:
    "Diese Art wird erfasst, nicht entschieden: Sie ist eingetragen, sobald Sie sie beantragen.",
  hrFieldFirstDay: "Erster freier Tag",
  hrFieldLastDay: "Letzter freier Tag",
  hrLastDayHint: "Der Tag, an dem Sie zurückkommen, gehört nicht dazu.",
  hrRangeBackwards: "Der letzte Tag liegt vor dem ersten.",
  hrAlsoAway: "Dann schon abwesend",
  hrNobodyAway: "Sonst ist an diesen Tagen niemand abwesend.",
  hrWhyHint:
    "Optional. Nur wer entscheidet, liest es, und protokolliert wird es nie.",
  hrAwayCalendar: "Wer abwesend ist, nach Tag",
  hrPreviousMonth: "Der Monat davor",
  hrNextMonth: "Der Monat danach",
  hrThisMonth: "Dieser Monat",
  hrAwayThisMonth: (count: number) =>
    count === 1
      ? "1 Person diesen Monat abwesend"
      : `${count} Personen diesen Monat abwesend`,
  hrMoreAway: (count: number) => `+${count} weitere`,
  hrDayAway: (day: string, count: number) =>
    count === 0 ? `${day}: niemand abwesend` : `${day}: ${count} abwesend`,
  hrNobodyAwayTitle: (month: string) => `Im ${month} ist niemand abwesend`,
  hrNobodyAwayBody:
    "Eingetragene Abwesenheiten erscheinen hier für alle im Unternehmen, damit Sie sehen, wer fehlt, bevor Sie um jemanden herum planen. Feiertage sind ebenfalls markiert.",

  // alo Kampagnen — die Zielgruppe und die Briefe. Die wichtigsten Wörter
  // sind die, die benennen, wer NICHT angeschrieben wird, und warum.
  campaignsTitle: "Zielgruppe",
  campaignsSubtitle:
    "Alle, die dieser Arbeitsbereich erreichen könnte — und alle, die er nicht erreichen darf, mit dem Grund.",
  campaignsCountriesLabel: "Länder",
  campaignsCountriesHint:
    "Zweibuchstabige Codes, durch Kommas getrennt. Leer heißt überall.",
  campaignsCountriesPlaceholder: "DE, AT",
  campaignsPurchaseLabel: "Käufe",
  campaignsPurchaseAny: "Egal",
  campaignsPurchaseBought: "Hat gekauft",
  campaignsPurchaseNotBought: "Hat nicht gekauft",
  campaignsPeriodLabel: "In den letzten",
  campaignsPeriodEver: "Jemals",
  campaignsPeriodDays: (days: number) => `${days} Tagen`,
  campaignsEveryone: "Alle",
  campaignsSegmentsLabel: "Gespeicherte Fragen",
  campaignsSaveSegment: "Diese Frage speichern",
  campaignsSegmentNamePrompt: "Wie soll diese Frage heißen?",
  campaignsSegmentNamePlaceholder: "Belgische Kunden",
  campaignsDeleteSegment: "Löschen",
  campaignsDeleteSegmentConfirm: (name: string) =>
    `Die Frage „${name}“ löschen? Niemandes Einwilligung oder Abmeldung wird angerührt — nur die Frage geht.`,
  campaignsTallyMailable: (mailable: number, matched: number) =>
    `${mailable} von ${matched} Personen werden angeschrieben`,
  campaignsTallyNobody:
    "Niemand in diesem Arbeitsbereich passt zu dieser Frage.",
  campaignsExcludedCount: (people: number, reason: string) =>
    `${people} · ${reason}`,
  campaignsWillBeMailed: "Werden angeschrieben",
  campaignsReasonNoConsent: "Nie eingewilligt",
  campaignsReasonUnsubscribe: "Abgemeldet",
  campaignsReasonHardBounce: "Mail unzustellbar",
  campaignsReasonComplaint: "Als Spam gemeldet",
  campaignsReasonManual: "Bat uns aufzuhören",
  campaignsTableLabel: "Personen, die diese Frage auswählt",
  campaignsColPerson: "Person",
  campaignsColCountry: "Land",
  campaignsColKnownFrom: "Bekannt aus",
  campaignsColStatus: "Status",
  campaignsSourceBillingCustomer: "Kunde",
  campaignsSourceCrmDeal: "Deal",
  campaignsSourceSiteForm: "Website-Formular",
  campaignsNoMatches: "Niemand passt zu dieser Frage.",
  campaignsMore: "Mehr Personen anzeigen",
  campaignsLoadFailed: "Die Zielgruppe konnte nicht gelesen werden.",
  campaignsSegmentsFailed:
    "Ihre gespeicherten Fragen konnten nicht gelesen werden.",
  campaignsSaveFailed: "Diese Frage konnte nicht gespeichert werden.",
  campaignsDeleteFailed: "Diese Frage konnte nicht entfernt werden.",
  campaignsEmptyTitle: "Noch niemand zu erreichen",
  campaignsEmptyBody:
    "Personen erscheinen hier, sobald dieser Arbeitsbereich einen Kunden hat, einen Deal mit E-Mail-Adresse, oder jemanden, der ein Formular auf seiner Website ausgefüllt hat. Persönliche Adressbücher werden nie verwendet.",
  campaignsNothingSentYet:
    "Von diesem Bildschirm wird nichts gesendet. Kampagnenversand braucht eine eigene Adresse, getrennt von Ihrer alltäglichen Post — damit ein Newsletter nie beeinflussen kann, ob Ihre Rechnungen ankommen.",
  campaignsViewsLabel: "Was ansehen",
  campaignsTabAudience: "Zielgruppe",
  campaignsTabLetters: "Briefe",
  campaignsLettersTitle: "Briefe",
  campaignsLettersSubtitle:
    "Jeder Brief so, wie eine Person ihn tatsächlich erhält.",
  campaignsLetterLabel: "Brief",
  campaignsNoLettersTitle: "Noch keine Briefe",
  campaignsNoLettersBody:
    "Ein Brief wird im selben Editor geschrieben wie ein Dokument: Überschriften, Absätze, Tabellen und Code. Sobald einer existiert, erscheint er hier — gerendert genau so, wie er ankommt.",
  campaignsShowAsLabel: "Anzeigen als",
  campaignsShowAsHint:
    "Beides ist echt. Bei der Hälfte einer Zielgruppe ist kein Name hinterlegt.",
  campaignsShowAsRecipient: "Jemand, den Sie anschreiben können",
  campaignsShowAsFallbacks: "Jemand ohne hinterlegte Angaben",
  campaignsPartLabel: "Teil",
  campaignsPartHint:
    "Jeder Brief trägt beide. Manche Menschen — und jeder Filter — lesen die schlichte Fassung.",
  campaignsPartHtml: "Formatiert",
  campaignsPartText: "Nur Text",
  campaignsPreviewFrameLabel: "Der Brief, wie er ankommt",
  campaignsPreviewSubject: "Betreff",
  campaignsPreviewPreheader: "Vorschautext",
  campaignsPreviewNoPreheader:
    "Keiner — Mailprogramme zeigen stattdessen die erste Zeile des Briefs.",
  campaignsAgainstRecipient: (person: string) =>
    `Das ist die Fassung, die ${person} erhält.`,
  campaignsAgainstFallbacks:
    "Das ist die Fassung für alle ohne hinterlegte Angaben — jeder personalisierte Wert unten ist Ihre eigene Ersatzformulierung.",
  campaignsAgainstNobodyYet:
    "Noch gibt es niemanden anzuschreiben; das ist deshalb die Fassung für jemanden ohne hinterlegte Angaben. Jeder personalisierte Wert unten ist Ihre eigene Ersatzformulierung.",
  campaignsPreviewCaveat:
    "Das ist die Meinung unseres Renderers, kein Beweis. Outlook unter Windows zeichnet Mail mit Words Engine, und jedes Programm weicht ab — legen Sie eine Testkopie in Ihre Entwürfe und lesen Sie sie dort, wo Ihre Empfänger es tun.",
  campaignsTestDraft: "Testkopie in meine Entwürfe legen",
  campaignsTestDraftDone: (address: string) =>
    `Eine Kopie liegt in Ihren Entwürfen, adressiert an ${address}. Gesendet wurde nichts — öffnen Sie sie in Ihrem Mailprogramm, oder senden Sie sie an sich selbst, um zu sehen, wie ein echtes Programm sie zeichnet.`,
  campaignsTestDraftFailed: "Diese Testkopie konnte nicht geschrieben werden.",
  campaignsFieldsTitle: "Was aus den personalisierten Werten wurde",
  campaignsColField: "Wert",
  campaignsColPrinted: "Liest sich als",
  campaignsColWhoseWords: "Wessen Worte",
  campaignsFieldTheirs: "Aus dem Datensatz der Person",
  campaignsFieldFallback: "Ihre Ersatzformulierung",
  campaignsNoFields: "Dieser Brief sagt allen dasselbe.",
  campaignsFieldFirstName: "Vorname",
  campaignsFieldName: "Vollständiger Name",
  campaignsFieldEmail: "E-Mail-Adresse",
  campaignsFieldCountry: "Land",
  campaignsVocabularyTitle: "Was Sie personalisieren können",
  campaignsFieldExample: (field: string) => `{{${field}|Ihre Worte}}`,
  campaignsVocabularyHint:
    "Die Worte nach dem Strich liest, wer nichts hinterlegt hat. Sie sind nicht optional: Ein Wert ohne Ersatz ist der Ort, wo „Hallo ,“ herkommt.",
  campaignsLettersFailed: "Ihre Briefe konnten nicht gelesen werden.",
  campaignsPreviewFailed: "Dieser Brief konnte nicht gerendert werden.",
  // Die Seite hinter einem Abmeldelink — der eine Bildschirm, den eine
  // fremde Person liest, und sie kommt bereits verärgert an. Kein Wort
  // nennt die Adresse; jeder Satz sagt genau, was ein Druck getan hat.
  campaignUnsubscribeLoading: "Dieser Link wird geprüft…",
  campaignUnsubscribeTitle: "Diese E-Mails beenden",
  campaignUnsubscribeSubtitle: (topic: string) =>
    `Diese Nachricht wurde als „${topic}“ gesendet. Sie können nur diese Art beenden — oder alles.`,
  campaignUnsubscribeSubtitleUntopiced:
    "Sie können den E-Mail-Empfang von diesem Arbeitsbereich beenden. Ein Druck genügt.",
  campaignUnsubscribeStopTopic: (topic: string) =>
    `Senden Sie mir „${topic}“ nicht mehr`,
  campaignUnsubscribeStopAll: "Senden Sie mir gar nichts mehr",
  campaignUnsubscribeAlreadyStopped:
    "Diesem Arbeitsbereich wurde bereits gesagt, dass er Ihnen nicht mehr schreiben soll. Sie müssen nichts weiter tun.",
  campaignUnsubscribeAlreadyDeclined: (topic: string) =>
    `„${topic}“ haben Sie bereits beendet. Alles Übrige können Sie unten weiterhin beenden.`,
  campaignUnsubscribeDoneTitle: "Erledigt",
  campaignUnsubscribeLinkText: "Abmelden",
  campaignUnsubscribeDoneAll:
    "Dieser Arbeitsbereich wird Ihnen nicht mehr schreiben. Weiter ist nichts nötig.",
  campaignUnsubscribeDoneTopic: (topic: string) =>
    `„${topic}“ wird Ihnen nicht mehr gesendet.`,
  campaignUnsubscribeDoneTopicNote:
    "Andere E-Mail-Arten aus diesem Arbeitsbereich — Rechnungen und Antworten etwa — erreichen Sie weiterhin. Kommen Sie zu diesem Link zurück, um auch sie zu beenden.",
  campaignUnsubscribeFinalNote:
    "Von hier aus lässt sich das nicht rückgängig machen. Wenn Sie es sich anders überlegen, wenden Sie sich direkt an den Absender.",
  campaignUnsubscribeNoAccountNote:
    "Kein Konto und keine Anmeldung nötig. Diese Seite betrifft nur die Adresse, an die diese Nachricht ging.",
  campaignUnsubscribeUnknownTitle: "Dieser Link funktioniert nicht mehr",
  campaignUnsubscribeUnknownLink:
    "Wir erkennen diesen Abmeldelink nicht. Falls Sie ihn aus einer E-Mail kopiert haben, öffnen Sie den Link direkt aus der E-Mail — oder antworten Sie dem Absender und bitten Sie ihn aufzuhören.",
  campaignUnsubscribeFailed:
    "Das ließ sich gerade nicht speichern. Bitte drücken Sie den Knopf noch einmal.",
  // Sites (alo Sites, ADR 0036) — the builder/editor half: the site list,
  // generation, templates, the page editor and palette, images, theme,
  // languages, the blog desk, collaborators, the contact-form inbox, the
  // assistant, analytics, the attention map, results, history, scheduled
  // publishing and page passwords. The commerce half (catalog, shop,
  // booking, tickets, orders, domains) ships in the next tranche.
  sitesLoadFailed: "Ihre Websites konnten nicht geladen werden.",
  sitesSiteLoadFailed: "Diese Website konnte nicht geladen werden.",
  sitesSaveFailed: "Die Änderung konnte nicht gespeichert werden.",
  sitesCheckFailed: "Die Adresse konnte nicht geprüft werden.",
  sitesNewSite: "Neue Website",
  sitesNoSitesTitle: "Noch keine Websites",
  sitesNoSitesBody:
    "Bauen Sie eine Website für Ihr Geschäft und veröffentlichen Sie sie unter ihrer eigenen Adresse.",
  sitesColName: "Name",
  sitesColAddress: "Adresse",
  sitesColStatus: "Status",
  sitesStatusDraft: "Entwurf",
  sitesStatusLive: "Online",
  sitesNewSiteTitle: "Neue Website",
  sitesNewSiteSubtitle:
    "Beginnen Sie mit einer Beschreibung, oder wählen Sie eine der fertigen Vorlagen.",
  sitesStartingPoint: "Wie Sie anfangen",
  sitesGenerateChoice: "Aus einer Beschreibung erzeugen",
  sitesGenerateChoiceDescription:
    "Beschreiben Sie alo Ihr Geschäft und erhalten Sie einen bearbeitbaren ersten Entwurf.",
  sitesTemplateChoice: "Mit einer Vorlage beginnen",
  sitesTemplateChoiceDescription:
    "Wählen Sie ein fertiges Layout und passen Sie es selbst an.",
  sitesBusinessDescription: "Beschreiben Sie Ihr Geschäft",
  sitesBusinessDescriptionHint:
    "Sagen Sie, was Sie anbieten, für wen es ist und welchen Ton Sie wollen. Vor dem Veröffentlichen können Sie alles bearbeiten.",
  sitesBusinessDescriptionPlaceholder:
    "Eine Bäckerei im Viertel, die Sauerteigbrot und Festtagstorten für Familien aus der Nachbarschaft backt…",
  sitesGenerateSite: "Website erzeugen",
  sitesGenerating: "Ihr Entwurf wird vorbereitet…",
  sitesCreatingSite: "Website wird angelegt…",
  sitesGenerationFailed:
    "Ihr Entwurf konnte nicht vorbereitet werden. Prüfen Sie die Servermeldung und versuchen Sie es erneut.",
  sitesGenerationEmpty:
    "Der erzeugte Entwurf enthielt keine Seite. Versuchen Sie eine ausführlichere Beschreibung.",
  sitesGenerationUnavailable:
    "Das Erzeugen ist für diesen Arbeitsbereich nicht eingerichtet. Beginnen Sie mit einer leeren Website oder wählen Sie unten eine Vorlage.",
  sitesChooseTemplate: "Wählen Sie einen Ausgangspunkt",
  sitesBlankTemplate: "Leere Website",
  sitesBlankTemplateSummary:
    "Eine leere Startseite. Jeden Abschnitt wählen Sie selbst.",
  sitesTemplatePageCount: (count: number) =>
    count === 1 ? "1 Seite" : `${count} Seiten`,
  sitesTemplatesLoading: "Vorlagen werden geladen…",
  sitesTemplatesLoadFailed:
    "Die Vorlagen konnten nicht geladen werden. Sie können trotzdem mit einer leeren Website beginnen.",
  sitesTemplatePreviewTitle: (name: string) => `Vorschau von ${name}`,
  sitesTemplatePreviewPages: "Seiten in dieser Vorlage",
  sitesTemplatePreviewLoading: "Vorschau wird geladen…",
  sitesTemplatePreviewFailed:
    "Diese Vorschau konnte nicht geladen werden. Sie können die Website trotzdem aus dieser Vorlage anlegen.",
  sitesTemplatePreviewNote:
    "Ein Bild der Seite. Wechseln Sie oben die Seite; jedes Wort und jeder Abschnitt gehört danach Ihnen.",
  sitesBlankPreviewNote:
    "Sie beginnen mit einer leeren Startseite und fügen die Abschnitte hinzu, die Sie wollen.",
  sitesHomePageTitle: "Startseite",
  sitesAiChanges: "KI-Änderungen",
  sitesAiEditTitle: "Eine Seitenänderung beschreiben",
  sitesAiEditBody:
    "alo bereitet eine prüfbare Änderungsliste vor. Nichts ändert sich, bevor Sie zustimmen.",
  sitesAiInstruction: "Seitenänderung",
  sitesAiInstructionPlaceholder:
    "Mach die Begrüßung wärmer und schiebe die Kundenstimmen über die Preise…",
  sitesAiPropose: "Änderungen vorbereiten",
  sitesAiPreparing: "Änderungen werden vorbereitet…",
  sitesAiProposalTitle: "Vorgeschlagene Änderungen",
  sitesAiProposalCount: (count: number) =>
    count === 1
      ? "1 vorgeschlagene Änderung"
      : `${count} vorgeschlagene Änderungen`,
  sitesAiPreviewHint:
    "Vergleichen Sie die Seite vorher und nachher, und entscheiden Sie dann, was geschieht.",
  sitesAiPreviewCompare: "Vorgeschlagene Seitenänderungen vergleichen",
  sitesInlineTextHint:
    "Klicken Sie in der Vorschau auf einen Text, um ihn dort zu bearbeiten. Eingabe speichert, Escape stellt ihn zurück.",
  sitesInlineTextSaved: "Text aktualisiert.",
  sitesInlineTextUndone: "Textänderung rückgängig gemacht.",
  sitesInlineTextRedone: "Textänderung wiederhergestellt.",
  sitesInlineTextStale:
    "Dieser Text gehört zu einem Abschnitt, der inzwischen verschoben oder geändert wurde. Die Vorschau wurde aufgefrischt — versuchen Sie die Änderung erneut.",
  sitesUndoEdit: "Letzte Änderung rückgängig machen",
  sitesRedoEdit: "Letzte Änderung wiederherstellen",
  sitesSectionDragHint:
    "Ziehen Sie einen Abschnitt, um ihn zu verschieben — die Seite ordnet sich beim Ziehen neu. Mit der Tastatur: den Abschnitt fokussieren und Alt mit der Pfeiltaste nach oben oder unten halten.",
  sitesSectionResizeHint:
    "Manche Abschnitte können ihre Form ändern. Wählen Sie in der Liste eine Größe unter dem Abschnitt, oder fokussieren Sie ihn in der Vorschau und halten Sie Alt mit der Pfeiltaste nach links oder rechts.",
  sitesLayoutOf: (control: string) => `${control} wählen`,
  sitesSectionResized: (section: string, choice: string) =>
    `${section} auf „${choice}“ gestellt.`,
  sitesLayoutSplit: "Aufteilung",
  sitesLayoutColumns: "Spalten",
  sitesLayoutShape: "Form",
  sitesLayoutSplitWideImage: "Bild breiter",
  sitesLayoutSplitHalf: "Gleiche Hälften",
  sitesLayoutSplitWideText: "Text breiter",
  sitesLayoutColumnsTwo: "Zwei",
  sitesLayoutColumnsThree: "Drei",
  sitesLayoutColumnsFour: "Vier",
  sitesLayoutShapeNatural: "Wie hochgeladen",
  sitesLayoutShapeWide: "Breit",
  sitesLayoutShapeSquare: "Quadratisch",
  sitesLayoutShapeTall: "Hoch",
  sitesSectionOnPage: (section: string, position: number, total: number) =>
    `${section}, Abschnitt ${position} von ${total}. Ziehen Sie ihn, um ihn zu verschieben, oder halten Sie Alt und drücken Sie die Pfeiltaste nach oben oder unten.`,
  sitesAiPreviewBefore: "Vorher",
  sitesAiPreviewAfter: "Nachher",
  sitesAiApprove: "Änderungen übernehmen",
  sitesAiApplying: "Änderungen werden übernommen…",
  sitesAiDiscard: "Verwerfen",
  sitesAiEditFailed:
    "Die Änderungsliste konnte nicht vorbereitet werden. Versuchen Sie es erneut oder bearbeiten Sie die Abschnitte direkt.",
  sitesAiApplyFailed:
    "Diese Änderungen konnten nicht übernommen werden. Prüfen Sie die Servermeldung und versuchen Sie es erneut.",
  sitesAiAddChange: (section: string, position: number) =>
    `${section} an Position ${position} einfügen`,
  sitesAiRemoveChange: (section: string) => `${section} entfernen`,
  sitesAiMoveChange: (section: string, position: number) =>
    `${section} an Position ${position} verschieben`,
  sitesAiSettingChange: (section: string) =>
    `Eine Einstellung in ${section} ändern`,
  sitesAiCopyChange: (section: string) => `Text in ${section} neu schreiben`,
  sitesAiImproveCopy: "Diesen Text verbessern",
  sitesAiCopyActions: "Textverbesserungen",
  sitesAiRewrite: "Neu schreiben",
  sitesAiShorter: "Kürzen",
  sitesAiLonger: "Ausführlicher machen",
  sitesAiTone: "Gewünschter Ton",
  sitesAiTonePlaceholder: "Warm und direkt",
  sitesAiUseTone: "Ton ändern",
  sitesAiCopyBefore: "Bisheriger Text",
  sitesAiCopyAfter: "Vorgeschlagener Text",
  sitesAiCopyFailed:
    "Diese Textänderung konnte nicht vorbereitet werden. Versuchen Sie es erneut oder bearbeiten Sie den Text direkt weiter.",
  sitesFieldName: "Name der Website",
  sitesFieldSubdomain: "Adresse",
  sitesSubdomainHint:
    "Kleinbuchstaben, Ziffern und Bindestriche, 3–40 Zeichen — daraus wird die Webadresse der Website.",
  sitesSubdomainChecking: "Verfügbarkeit wird geprüft…",
  sitesSubdomainAvailable: (subdomain: string) => `„${subdomain}“ ist frei.`,
  sitesSubdomainTaken: (subdomain: string) =>
    `„${subdomain}“ ist bereits vergeben.`,
  sitesAddressAvailable: "Frei",
  sitesAddressTaken: "Bereits vergeben",
  sitesAddressNotChecked:
    "Geben Sie eine gültige Adresse ein, um die Verfügbarkeit zu prüfen",
  sitesNameRequired: "Geben Sie Ihrer Website einen Namen, um fortzufahren.",
  sitesAddressRequired: "Geben Sie eine Adresse ein, um fortzufahren.",
  sitesCreateSite: "Website anlegen",
  sitesCancel: "Abbrechen",
  sitesBack: "Alle Websites",
  sitesCollaborators: "Mitwirkende",
  sitesCollaboratorsHint:
    "Laden Sie Personen ein, diese Website zu bearbeiten und zu veröffentlichen. Ihre E-Mails, Dateien und anderen Websites können sie nicht öffnen.",
  sitesCollaboratorEmail: "E-Mail-Adresse",
  sitesCollaboratorEmailPlaceholder: "mitwirkende@example.com",
  sitesInviteCollaborator: "Zum Bearbeiten einladen",
  sitesCollaboratorsLoading: "Mitwirkende werden geladen…",
  sitesCollaboratorsLoadFailed:
    "Die Mitwirkenden dieser Website konnten nicht geladen werden.",
  sitesCollaboratorInviteFailed: "Die Einladung konnte nicht angelegt werden.",
  sitesCollaboratorRevokeFailed: "Der Zugang konnte nicht entfernt werden.",
  sitesCollaboratorCopyFailed:
    "Der Einrichtungslink konnte nicht kopiert werden. Erstellen Sie einen neuen Link und versuchen Sie es erneut.",
  sitesCollaboratorLinkReady: (email: string) =>
    `Ein privater Einrichtungslink für ${email} liegt bereit. Kopieren Sie ihn und geben Sie ihn auf sicherem Weg weiter.`,
  sitesCollaboratorAdded: (email: string) =>
    `${email} kann diese Website jetzt bearbeiten.`,
  sitesCollaboratorLinkCopied: "Einrichtungslink kopiert.",
  sitesCollaboratorRevoked: (email: string) =>
    `Der Zugang von ${email} wurde entfernt.`,
  sitesUndoCollaboratorRevoke: "Rückgängig",
  sitesNoCollaborators:
    "Nur Sie können diese Website bearbeiten. Geben Sie oben eine E-Mail-Adresse ein, um die erste Person einzuladen.",
  sitesCollaboratorPending: "Einladung offen",
  sitesCollaboratorActive: "Kann bearbeiten und veröffentlichen",
  sitesRefreshCollaboratorLink: "Neuer Einrichtungslink",
  sitesCopyCollaboratorLink: "Einrichtungslink kopieren",
  sitesRevokeCollaborator: "Zugang entfernen",
  sitesInvitationHeading: "Dieser Website beitreten",
  sitesInvitationSubtitle: (site: string) =>
    `Sie sind eingeladen, ${site} zu bearbeiten und zu veröffentlichen.`,
  sitesInvitationLoading: "Ihre Einladung wird geprüft…",
  sitesInvitationLoadFailed:
    "Diese Einladung ist abgelaufen oder wurde bereits verwendet. Bitten Sie die Person, der die Website gehört, um einen neuen Link.",
  sitesInvitationPassword: "Passwort festlegen",
  sitesInvitationPasswordHint: "Mindestens 8 Zeichen.",
  sitesInvitationConfirmPassword: "Passwort bestätigen",
  sitesInvitationPasswordMismatch: "Die Passwörter stimmen nicht überein.",
  sitesInvitationAccept: "Website beitreten",
  sitesInvitationAccepting: "Beitritt läuft…",
  sitesInvitationAcceptFailed: "Ihre Einladung konnte nicht angenommen werden.",
  sitesInvitationDone: "Sie können jetzt bearbeiten",
  sitesInvitationDoneBody: (email: string) =>
    `Melden Sie sich als ${email} an. Sie sehen nur die Websites, die mit Ihnen geteilt wurden.`,
  sitesInvitationSignIn: "Bei alo anmelden",
  sitesPages: "Seiten",
  sitesPageCount: (count: number) =>
    `${count} ${count === 1 ? "Seite" : "Seiten"}`,
  sitesOverview: "Uebersicht",
  sitesOverviewHealth: "Website-Status",
  sitesOverviewActions: "Letzte Aktivitaet",
  sitesOverviewReadiness: "Bereit fuer den Start",
  sitesOverviewReadinessHint:
    "Schliessen Sie die Grundlagen ab, bevor Sie die Website teilen.",
  sitesOverviewReadyCount: (ready: number, total: number) =>
    `${ready} von ${total} bereit`,
  sitesOverviewDomainStep: "Website-Adresse verbunden",
  sitesOverviewPagesStep: "Mindestens eine Seite erstellt",
  sitesOverviewLanguagesStep: "Aktivierte Sprachen vorbereitet",
  sitesOverviewPublishStep: "Website veroeffentlicht",
  sitesOverviewContinue: "Weiterarbeiten",
  sitesOverviewContinueHint: "Direkt zu den wichtigsten Website-Werkzeugen.",
  sitesOverviewReadinessScore: "Gesamtbereitschaft",
  sitesOverviewFoundation: "Grundlage",
  sitesOverviewContent: "Seiteninhalt",
  sitesOverviewLocalization: "Sprachen",
  sitesOverviewLaunch: "Veroeffentlichung",
  sitesOverviewElements: "Seitenelemente",
  sitesOverviewElementsHint:
    "Eine starke Startseite verbindet Struktur, klare Einfuehrung, nuetzliche Inhalte und eine Handlungsaufforderung.",
  sitesOverviewNavigationElements: "Navigation",
  sitesOverviewHeroElements: "Einfuehrung",
  sitesOverviewContentElements: "Inhalt",
  sitesOverviewActionElements: "Handlungsaufforderung",
  sitesOverviewSeo: "Sucheinstellungen",
  sitesOverviewAccessibility: "Barrierefreiheit",
  sitesOverviewBranding: "Markenidentitaet",
  sitesOverviewQuality: "SEO- und Markenqualitaet",
  sitesOverviewQualityHint:
    "Verbessern Sie Suchdarstellung, Bildverstaendlichkeit und den einheitlichen Markenauftritt Ihrer Website.",
  sitesOverviewSeoTitles: "SEO-Titel",
  sitesOverviewMetaDescriptions: "Meta-Beschreibungen",
  sitesOverviewImageDescriptions: "Bildbeschreibungen",
  sitesOverviewLogo: "Logo",
  sitesOverviewFavicon: "Favicon",
  sitesOverviewRecommendedNext: "Empfohlener naechster Schritt",
  sitesOverviewAddIntroduction: "Klare Einfuehrung hinzufuegen",
  sitesOverviewAddContent: "Nuetzliche Seiteninhalte hinzufuegen",
  sitesOverviewAddAction: "Handlungsaufforderung hinzufuegen",
  sitesOverviewEditPage: "Seiteneditor oeffnen",
  sitesSiteTools: "Website-Einstellungen",
  sitesSiteSettings: "Website-Einstellungen",
  sitesSiteToolsHint:
    "Domains, SEO-Standards, Weiterleitungen, Statistiken und Code",
  sitesSiteSettingsHint:
    "Domains, Versionsverlauf, Statistiken, Shop, Formulare und Automatisierung.",
  sitesPublishing: "Veröffentlichen",
  sitesWebsiteNavigation: "Website-Einstellungen",
  sitesNewPage: "Neue Seite",
  sitesNoPagesTitle: "Noch keine Seiten",
  sitesNoPagesBody:
    "Jede Website beginnt mit einer Startseite. Legen Sie eine an, um loszubauen.",
  sitesColPage: "Seite",
  sitesColPath: "Pfad",
  sitesColSeo: "Suche",
  sitesColAccess: "Zugriff",
  sitesSearchPages: "Seiten suchen",
  sitesPageFilter: "Seitenfilter",
  sitesFilterAllPages: "Alle",
  sitesFilterHomePage: "Startseite",
  sitesFilterProtectedPages: "GeschÃ¼tzt",
  sitesSeoReady: "Bereit",
  sitesSeoNeedsWork: "Offen",
  sitesPublicPage: "Ã–ffentlich",
  sitesNoMatchingPages: "Keine Seiten in dieser Ansicht.",
  sitesPosts: "Blogartikel",
  sitesSortPages: "Sortieren",
  sitesSortNavigation: "Navigation",
  sitesSortName: "Name",
  sitesSortPath: "Pfad",
  sitesEditPage: "Bearbeiten",
  sitesPageActions: "Seitenaktionen",
  sitesExpandChildPages: "Unterseiten einblenden",
  sitesCollapseChildPages: "Unterseiten ausblenden",
  sitesRenamePage: "Umbenennen",
  sitesRenamePagePrompt:
    "Wählen Sie den Seitennamen, der in Alo und auf der Website angezeigt wird.",
  sitesDuplicatePage: "Duplizieren",
  sitesSetHomepage: "Als Startseite festlegen",
  sitesSetHomepageConfirm: (page: string) =>
    `${page} als Startseite festlegen?`,
  sitesDeletePage: "Löschen",
  sitesDeletePageConfirm: (page: string) =>
    `${page} löschen? Dadurch wird die Entwurfsseite von dieser Website entfernt.`,
  sitesPageActionFailed: "Die Seitenaktion konnte nicht abgeschlossen werden.",
  sitesLastEdited: (date: string) => `Aktualisiert ${date}`,
  sitesStatusPublished: "Veroeffentlicht",
  sitesViewSite: "Website ansehen",
  sitesDomainHealthy: "Domain bereit",
  sitesNotPublishedYet: "Noch nicht veroeffentlicht",
  sitesLastPublished: (date: string) => `Zuletzt veroeffentlicht: ${date}`,
  sitesBackToWebsite: "Website",
  sitesPostsLoadFailed: "Ihre Blogartikel konnten nicht geladen werden.",
  sitesLoadingPosts: "Blogartikel werden geladen",
  sitesWriteInDocs: "In alo Docs schreiben",
  sitesOpeningDocs: "alo Docs wird geöffnet…",
  sitesUntitledArticle: "Artikel ohne Titel",
  sitesPostCreateFailed:
    "Der Artikel konnte nicht angelegt werden. Versuchen Sie es erneut.",
  sitesNoPostsTitle: "Noch keine Artikel",
  sitesNoPostsBody:
    "Beginnen Sie einen Artikel in alo Docs. Er bleibt privat, bis Sie ihn veröffentlichen.",
  sitesColArticle: "Artikel",
  sitesColUpdated: "Aktualisiert",
  sitesColActions: "Aktionen",
  sitesEditInDocs: "In alo Docs bearbeiten",
  sitesPostStatusDraft: "Entwurf",
  sitesPostStatusPublished: "Veröffentlicht",
  sitesPublishArticle: "Veröffentlichen",
  sitesPublishArticleTitle: "Artikel veröffentlichen",
  sitesPublishArticleSubtitle:
    "Legen Sie fest, wie der Artikel auf Ihrer öffentlichen Website erscheint.",
  sitesEditArticleTitle: "Artikeldetails",
  sitesEditArticleSubtitle:
    "Ändern Sie, was Ihre Leserschaft auf Ihrer Website sieht.",
  sitesEditArticleDetails: "Details bearbeiten",
  sitesSaveArticle: "Änderungen speichern",
  sitesPostSaveFailed:
    "Die Artikeldetails konnten nicht gespeichert werden. Versuchen Sie es erneut.",
  sitesPostUnpublishFailed:
    "Der Artikel konnte nicht vom Netz genommen werden. Versuchen Sie es erneut.",
  sitesUnpublishArticle: "Vom Netz nehmen",
  sitesUnpublishingArticle: "Wird vom Netz genommen…",
  sitesFieldPostTitle: "Artikeltitel",
  sitesFieldPostSlug: "Webadresse",
  sitesPostSlugHint: "Kleinbuchstaben, Ziffern und Bindestriche.",
  sitesPostSlugPlaceholder: "mein-artikel",
  sitesFieldPostExcerpt: "Zusammenfassung",
  sitesPostExcerptHint:
    "Eine kurze Einführung, gezeigt auf der Blogseite und im RSS-Feed.",
  sitesFieldPostCover: "Titelbild",
  sitesPostCoverHint: "Zu sehen auf der Blogseite und über dem Artikel.",
  sitesPostNoCover: "Kein Titelbild",
  sitesPostCoverAdded: "Titelbild hinzugefügt",
  sitesAddPostCover: "Bild hinzufügen",
  sitesReplacePostCover: "Bild ersetzen",
  sitesRemovePostCover: "Entfernen",
  sitesUploadingPostCover: "Wird hochgeladen…",
  sitesPostCoverUploadFailed:
    "Das Titelbild konnte nicht hochgeladen werden. Versuchen Sie es erneut.",
  sitesHomeBadge: "Startseite",
  sitesNewPageTitle: "Neue Seite",
  sitesNewPageSubtitle:
    "Eine Seite trägt die Abschnitte, die Sie auf ihr stapeln.",
  sitesFieldPageTitle: "Titel",
  sitesFieldSlug: "Pfad",
  sitesLanguagesLabel: "Sprachen der Website",
  sitesEditingLanguage: "Bearbeitungssprache",
  sitesLanguages: "Sprachen",
  sitesLanguagesHint:
    "Waehlen Sie die Inhaltssprachen dieser Website. Das aendert nicht die alo-Sprache der Bearbeiter.",
  sitesDefaultLanguage: "Standardsprache",
  sitesAddLanguage: "Eine Sprache hinzufügen",
  sitesLanguagePlaceholder: "Sprachcode, zum Beispiel zh-Hans",
  sitesAddLanguageAction: "Sprache hinzufügen",
  sitesLanguageDefaultBadge: "Standard",
  sitesRemoveLanguage: (language: string) => `${language} entfernen`,
  sitesLanguageSaveFailed:
    "Die Sprachen der Website konnten nicht gespeichert werden. Prüfen Sie den Sprachcode und versuchen Sie es erneut.",
  sitesTranslationReady: "Bereit",
  sitesTranslationProgress: (translated: number, total: number) =>
    `${translated} von ${total} Seiten übersetzt`,
  sitesTranslationAllReady:
    "Jede aktivierte Sprache ist bereit zur Veröffentlichung.",
  sitesTranslationPublishHint: (count: number) =>
    count === 1
      ? "1 Übersetzung zeigt noch Inhalte der Ausgangssprache."
      : `${count} Übersetzungen zeigen noch Inhalte der Ausgangssprache.`,
  sitesContinueTranslating: "Weiter übersetzen",
  sitesTranslationSaveFailed:
    "Diese Übersetzung konnte nicht gespeichert werden. Korrigieren Sie die markierten Angaben und versuchen Sie es erneut.",
  sitesTranslationMissingTitle: (locale: string) =>
    `${locale} braucht eine Übersetzung`,
  sitesTranslationMissingBody: (requested: string, source: string) =>
    `Zur Orientierung sehen Sie die Fassung in ${source}. Kopieren Sie sie nach ${requested}, um mit dem Übersetzen zu beginnen, ohne die Ausgangsseite zu verändern.`,
  sitesCopyTranslation: (source: string, target: string) =>
    `${source} nach ${target} kopieren`,
  sitesTranslationDetails: "Details der übersetzten Seite",
  sitesTranslationDetailsHint: (locale: string) =>
    `Titel, Pfad und Suchangaben hier sehen nur Besucher in ${locale}.`,
  sitesSaveTranslation: "Übersetzungsdetails speichern",
  sitesTranslateWholeSite: "Ganze Website übersetzen",
  sitesWholeTranslationPreparing:
    "Eine vollständige Übersetzung wird zur Prüfung vorbereitet…",
  sitesWholeTranslationPrepareFailed:
    "Die Übersetzung konnte nicht vorbereitet werden. Nichts hat sich geändert; übersetzen Sie Seiten von Hand oder versuchen Sie es erneut.",
  sitesWholeTranslationApplyFailed:
    "Die Übersetzung konnte nicht übernommen werden. Nichts hat sich geändert; bereiten Sie eine frische Prüfung vor und versuchen Sie es erneut.",
  sitesWholeTranslationReview: (language: string) =>
    `Die Übersetzung in ${language} prüfen`,
  sitesWholeTranslationReviewHint:
    "Vergleichen Sie jede Seite und jeden Artikel. Nichts wird gespeichert, bevor Sie diese Prüfung freigeben.",
  sitesWholeTranslationApprove: "Übersetzung übernehmen",
  sitesTranslationPageKind: "Seite",
  sitesTranslationPostKind: "Artikel",
  sitesSlugHint:
    "Kleinbuchstaben, Ziffern und Bindestriche. Bei der Startseite bleibt das leer.",
  sitesFieldHome: "Das ist die Startseite",
  sitesCreatePage: "Seite anlegen",
  sitesPageLoadFailed: "Diese Seite konnte nicht geladen werden.",
  sitesBackToSite: "Alle Seiten",
  sitesSections: "Abschnitte",
  sitesAddSection: "Abschnitt hinzufügen",
  sitesAddFirstSection: "Den ersten Abschnitt hinzufügen",
  sitesNoSectionsTitle: "Noch nichts auf dieser Seite",
  sitesNoSectionsBody:
    "Stapeln Sie Abschnitte — einen Aufmacher, Ihre Leistungen, ein Kontaktformular — und die Seite entsteht.",
  sitesPaletteTitle: "Einen Abschnitt hinzufügen",
  sitesPaletteHint:
    "Wählen Sie einen Baustein und prüfen Sie ihn mit Ihren Inhalten, bevor Sie ihn hinzufügen.",
  sitesPaletteCategories: "Abschnittskategorien",
  sitesPaletteCategoryAll: "Alle",
  sitesPaletteCategoryEssentials: "Grundlagen",
  sitesPaletteCategoryContent: "Inhalte",
  sitesPaletteCategoryBusiness: "Geschäft",
  sitesPaletteCategoryAdvanced: "Erweitert",
  sitesPalettePosition: "Wohin er kommt",
  sitesPaletteAtTop: "Ganz oben",
  sitesPaletteAtEnd: "Ans Ende",
  sitesPaletteAfter: (section: string) => `Hinter „${section}“`,
  sitesPaletteAdd: (section: string, position: string) =>
    `${section} hinzufügen — ${position}`,
  sitesPaletteDropHere: "Hier ablegen, um am Ende einzufügen",
  sitesPaletteOwnContent: "Gezeigt mit Ihren eigenen Inhalten.",
  sitesPalettePreviewTitle: (section: string) => `${section} auf Ihrer Website`,
  sitesPaletteLoading:
    "Diese Bausteine füllen sich mit Ihren eigenen Inhalten…",
  sitesPaletteFailed:
    "Ihre eigenen Inhalte konnten nicht geladen werden, daher öffnen diese Bausteine ein Formular.",
  sitesPaletteOpensForm: "Öffnet ein Formular",
  sitesPaletteDone: "Fertig mit Hinzufügen",
  sitesPaletteNeedsWriting:
    "Hier gibt es noch nichts von Ihnen zu zeigen — diesen Baustein schreiben Sie selbst. Beim Hinzufügen öffnet sich ein Formular.",
  sitesPaletteNeedsPicture:
    "Legen Sie ein Bild auf diese Website, und dieser Baustein füllt sich damit. Jetzt hinzugefügt, öffnet er ein Formular.",
  sitesPaletteNeedsCatalog:
    "Legen Sie zuerst einen Katalog an — dieser Baustein zeigt, was darin ist. Jetzt hinzugefügt, öffnet er ein Formular.",
  sitesPaletteNeedsCollection:
    "Verbinden Sie zuerst eine Sammlung — dieser Baustein zeigt ihre Zeilen. Jetzt hinzugefügt, öffnet er ein Formular.",
  sitesPaletteNeedsBooking:
    "Legen Sie zuerst etwas an, das man buchen kann — dieser Baustein bietet es an. Jetzt hinzugefügt, öffnet er ein Formular.",
  sitesPaletteNeedsCode:
    "Den Code in diesem Baustein schreiben Sie selbst. Beim Hinzufügen öffnet sich ein Formular.",
  sitesAddSectionTitle: (section: string) => `${section} hinzufügen`,
  sitesEditSectionTitle: (section: string) => `${section} bearbeiten`,
  sitesSaveSection: "Abschnitt speichern",
  sitesSectionSaved: "Gespeichert",
  sitesMoveUp: (section: string) => `${section} nach oben schieben`,
  sitesMoveDown: (section: string) => `${section} nach unten schieben`,
  sitesEditSection: (section: string) => `${section} bearbeiten`,
  sitesDeleteSection: (section: string) => `${section} löschen`,
  sitesSectionMoved: (section: string, position: number, total: number) =>
    `${section} an Position ${position} von ${total} verschoben.`,
  sitesSectionAdded: (section: string, position: number, total: number) =>
    `${section} als Abschnitt ${position} von ${total} hinzugefügt.`,
  sitesConfirmDelete: "Wirklich löschen?",
  sitesPreview: "Vorschau",
  sitesShowPreview: "Vorschau anzeigen",
  sitesHidePreview: "Vorschau ausblenden",
  sitesResizeWorkspace:
    "Breite der Bereiche Abschnitte und Vorschau ändern (ziehen oder die Pfeiltasten links und rechts verwenden; zum Zurücksetzen doppelklicken)",
  sitesPreviewTitle: "Entwurfsvorschau",
  sitesPreviewDesktop: "Bildschirmbreite",
  sitesPreviewMobile: "Telefonbreite",
  sitesPreviewFailed: "Die Vorschau konnte nicht geladen werden.",
  sitesSeoAction: "Suche & Teilen",
  sitesSeoTitle: "Suche & Teilen",
  sitesSeoSubtitle:
    "Legen Sie fest, wie diese Seite in Suchergebnissen und geteilten Links erscheint.",
  sitesSeoPreview: "Vorschau des Suchergebnisses",
  sitesSeoFieldTitle: "Suchtitel",
  sitesSeoTitleHint:
    "Leer lassen, um Seitentitel und Website-Namen zu verwenden.",
  sitesSeoFieldDescription: "Beschreibung",
  sitesSeoDescriptionHint:
    "Eine kurze, nützliche Zusammenfassung für Suchergebnisse und geteilte Links.",
  sitesSeoDescriptionDefault:
    "Fügen Sie eine Beschreibung hinzu, damit man weiß, worum es auf dieser Seite geht.",
  sitesSeoImageHint:
    "Geteilte Links verwenden das erste Aufmacherbild der Seite. Gibt es keines, wird Ihr Website-Logo verwendet.",
  sitesSeoSave: "Suchangaben speichern",
  sitesSeoSaveFailed:
    "Die Suchangaben konnten nicht gespeichert werden. Versuchen Sie es erneut.",
  sitesSectionNav: "Navigationsleiste",
  sitesNavigation: "Navigation",
  sitesSectionNavDesc: "Links quer über den Seitenkopf.",
  sitesSectionHero: "Aufmacher",
  sitesSectionHeroDesc: "Die große Schlagzeile am Anfang.",
  sitesSectionFeatures: "Leistungen",
  sitesSectionFeaturesDesc: "Ein Raster mit dem, was Sie anbieten.",
  sitesSectionTextImage: "Text & Bild",
  sitesSectionTextImageDesc: "Ein Absatz neben einem Bild.",
  sitesSectionGallery: "Galerie",
  sitesSectionGalleryDesc: "Eine Wand voller Bilder.",
  sitesSectionTestimonials: "Kundenstimmen",
  sitesSectionTestimonialsDesc: "Worte zufriedener Kundschaft.",
  sitesSectionPricing: "Preise",
  sitesSectionPricingDesc: "Ihre Pakete und ihre Preise.",
  sitesSectionTeam: "Team",
  sitesSectionTeamDesc: "Die Menschen hinter dem Geschäft.",
  sitesSectionFaq: "FAQ",
  sitesSectionFaqDesc: "Fragen, die gestellt werden — beantwortet.",
  sitesSectionCta: "Handlungsaufruf",
  sitesSectionCtaDesc: "Ein Banner, das um den Klick bittet.",
  sitesSectionContactForm: "Kontaktformular",
  sitesSectionContactFormDesc: "Besucher können Ihnen schreiben.",
  sitesSectionFooter: "Fußzeile",
  sitesSectionFooterDesc: "Die Zeile ganz unten auf der Seite.",
  sitesCountLinks: (count: number) =>
    count === 1 ? "1 Link" : `${count} Links`,
  sitesNavPinned: "Oben angeheftet",
  sitesNavEditorIntroTitle: "Eine klare Kopfzeile erstellen",
  sitesNavEditorIntro:
    "Ihr Logo oder Sitename kommt aus dem Design. Halten Sie das Hauptmenü übersichtlich und verwenden Sie eine Schaltfläche für die wichtigste Aktion.",
  sitesNavMenuLinks: "Menülinks",
  sitesNavSettings: "Navigationseinstellungen",
  sitesNavEditorTabs: "Navigationseditor",
  sitesNavLinksTab: "Links",
  sitesNavSettingsTab: "Einstellungen",
  sitesNavSettingsHint:
    "Menülinks, primäre Aktion und Erscheinungsbild zentral verwalten.",
  sitesNavMenuLinksHint:
    "Fügen Sie die wichtigsten Seiten hinzu. Die Reihenfolge hier entspricht der Reihenfolge in der Kopfzeile.",
  sitesNavAddPages: "Siteseiten hinzufügen",
  sitesNavPagesLoading: "Seiten werden geladen…",
  sitesNavChoosePage: "Siteseite auswählen",
  sitesNavDestination: "Seite oder Abschnitt",
  sitesNavPages: "Seiten",
  sitesNavSections: "Abschnitte",
  sitesNavCustomTarget: "Eigener Link",
  sitesNavDestinationHint:
    "Verwenden Sie einen Seitenpfad (/ueber-uns), Abschnitt (#preise), eine Webadresse, E-Mail oder Telefonnummer.",
  sitesNavPrimaryAction: "Hauptaktion",
  sitesNavPrimaryActionHint:
    "Optional. Sie erscheint als hervorgehobene Schaltfläche in der Kopfzeile.",
  sitesNavPagesLoadFailed:
    "Die Siteseiten konnten nicht geladen werden. Sie können Links weiterhin manuell eingeben.",
  sitesNavMoveLinkUp: (position: number) =>
    `Link ${position} nach oben verschieben`,
  sitesNavMoveLinkDown: (position: number) =>
    `Link ${position} nach unten verschieben`,
  sitesNavAlreadyAdded: "Bereits auf dieser Seite",
  sitesNavAppearance: "Erscheinungsbild",
  sitesNavAppearanceShow: "Navigationserscheinungsbild anzeigen",
  sitesNavUsesTheme: "Verwendet das Website-Theme",
  sitesNavUsesBrandRoles: "Verwendet wiederverwendbare Markenfarben",
  sitesNavBrandRoleHint:
    "Wählen Sie Farben aus dem Website-Theme. Eine Änderung der Markenpalette aktualisiert alle verbundenen Abschnitte.",
  sitesNavResetRoles: "Standardwerte verwenden",
  sitesNavCustomPalette: "Eigene Navigationsfarben",
  sitesNavPaletteChoices: "Navigationsfarbstile",
  sitesNavPaletteTheme: "Website-Theme",
  sitesNavPaletteLight: "Hell",
  sitesNavPaletteDark: "Dunkel",
  sitesNavPaletteCustom: "Eigene Marke",
  sitesNavCustomHint:
    "Wählen Sie eine beliebige Farbe oder geben Sie den exakten HEX-Markenwert ein.",
  sitesNavHexValue: (label: string) => `${label}-HEX-Wert`,
  sitesNavBackground: "Hintergrund",
  sitesNavText: "Text",
  sitesNavHover: "Hover",
  sitesNavPreviewBrand: "Ihre Marke",
  sitesNavPreviewLink: "Menülink",
  sitesNavPreviewHover: "Hover-Zustand",
  sitesNavContrastPass: "Text- und Hoverfarben erfüllen WCAG AA.",
  sitesNavContrastFail: "Wählen Sie mindestens 4,5:1 Kontrast.",
  sitesCountImages: (count: number) =>
    count === 1 ? "1 Bild" : `${count} Bilder`,
  sitesCountEntries: (count: number) =>
    count === 1 ? "1 Eintrag" : `${count} Einträge`,
  sitesItemN: (position: number) => `Eintrag ${position}`,
  sitesRemoveItem: "Eintrag entfernen",
  sitesAddLink: "Link hinzufügen",
  sitesAddEntry: "Eintrag hinzufügen",
  sitesAddImage: "Bild hinzufügen",
  sitesAddTier: "Paket hinzufügen",
  sitesAddMember: "Person hinzufügen",
  sitesAddQuestion: "Frage hinzufügen",
  sitesHeroLayout: "Layout",
  sitesHeroLayoutHint:
    "Wählen Sie die Anordnung, die Überschrift und Bild am besten unterstützt.",
  sitesHeroLayoutCentered: "Zentriert",
  sitesHeroLayoutSplitRight: "Bild rechts",
  sitesHeroLayoutSplitLeft: "Bild links",
  sitesHeroLayoutBackground: "Hintergrundbild",
  sitesHeroLayoutEditorial: "Editorial",
  sitesHeroDesign: "Design",
  sitesHeroHeight: "Höhe",
  sitesHeroHeightCompact: "Kompakt",
  sitesHeroHeightStandard: "Standard",
  sitesHeroHeightTall: "Hoch",
  sitesHeroAlignment: "Inhaltsausrichtung",
  sitesHeroAlignmentLeft: "Links",
  sitesHeroAlignmentCenter: "Mittig",
  sitesHeroAlignmentRight: "Rechts",
  sitesHeroContentWidth: "Textbreite",
  sitesHeroContentWidthNarrow: "Schmal",
  sitesHeroContentWidthBalanced: "Ausgewogen",
  sitesHeroContentWidthWide: "Breit",
  sitesHeroContent: "Inhalt",
  sitesHeroMedia: "Bild",
  sitesHeroActions: "Buttons",
  sitesFieldHeading: "Überschrift",
  sitesFieldSubheading: "Unterzeile",
  sitesFieldIntro: "Einleitung",
  sitesFieldBody: "Text",
  sitesFieldItemTitle: "Titel",
  sitesFieldLinkLabel: "Linktext",
  sitesFieldLinkHref: "Linkziel",
  sitesFieldButton: "Knopf",
  sitesFieldPrimaryButton: "Erster Knopf",
  sitesFieldSecondaryButton: "Zweiter Knopf",
  sitesFieldImage: "Bild",
  sitesFieldPhoto: "Foto",
  sitesFieldImageId: "Bild-ID",
  sitesImageIdHint:
    "Laden Sie ein Bild hoch, oder fügen Sie die Bild-ID eines früheren Uploads ein.",
  sitesFieldImageAlt: "Bildbeschreibung",
  sitesImageAltHint:
    "Wird von Screenreadern vorgelesen. Sagen Sie, was das Bild zeigt; zeigt es nichts, worauf es ankommt, markieren Sie es unten als dekorativ.",
  sitesImageAltMissing:
    "Dieses Bild hat noch keine Beschreibung — sagen Sie, was es zeigt, oder markieren Sie es als dekorativ.",
  sitesImageDecorative: "Dekorativ — Screenreader überspringen es",
  sitesImageDecorativeHint:
    "Nur für Bilder, die selbst nichts mitteilen, etwa ein Hintergrundmuster.",
  sitesImageFrameHint:
    "Ziehen Sie auf dem Bild, um zu wählen, was sichtbar bleibt. Mit der Tastatur: Pfeiltasten verschieben den Rahmen, Umschalt mit den Pfeiltasten ändert seine Größe.",
  sitesImageFocalHint:
    "Ziehen Sie die runde Markierung auf das, was im Blick bleiben muss, wenn ein Layout das Bild weiter beschneiden muss.",
  sitesImageFrameAt: (
    width: number,
    height: number,
    left: number,
    top: number,
  ) =>
    `Sichtbarer Ausschnitt: ${width} % mal ${height} % des Bildes, ${left} % von links und ${top} % von oben`,
  sitesImageFocalAt: (x: number, y: number) =>
    `Fokuspunkt ${x} % von links, ${y} % von oben`,
  sitesImageFrameWidth: "Breite",
  sitesImageFrameHeight: "Höhe",
  sitesImageFrameLeft: "Links",
  sitesImageFrameTop: "Oben",
  sitesImageWholePicture: "Das ganze Bild verwenden",
  sitesImageWholePictureState: "Das ganze Bild ist zu sehen",
  sitesImageCentreFocal: "Fokuspunkt in die Mitte",
  sitesImageNoPreview:
    "Dieses Bild kann hier nicht angezeigt werden. Die Zahlen darunter rahmen es trotzdem, und seine Beschreibung bleibt unberührt.",
  sitesAiAltWrite: "Beschreibung vorschlagen",
  sitesAiAltImprove: "Diese Beschreibung verbessern",
  sitesAiAltProposed: "Vorgeschlagene Beschreibung",
  sitesAiAltUnseen:
    "Entworfen aus den Worten dieses Abschnitts — alo hat das Bild nicht gesehen. Prüfen Sie den Vorschlag am Bild, bevor Sie ihn übernehmen.",
  sitesAiAltFailed: "Die Beschreibung konnte nicht entworfen werden.",
  sitesFieldImageSide: "Bildseite",
  sitesSideLeft: "Links",
  sitesSideRight: "Rechts",
  sitesFieldQuote: "Zitat",
  sitesFieldAuthor: "Name",
  sitesFieldRole: "Rolle",
  sitesFieldTierName: "Paketname",
  sitesFieldPrice: "Preis",
  sitesFieldPeriod: "Abrechnungszeitraum",
  sitesFieldTierDescription: "Beschreibung",
  sitesFieldTierFeatures: "Was enthalten ist",
  sitesTierFeaturesHint: "Eine Zeile je Punkt.",
  sitesFieldHighlighted: "Dieses Paket hervorheben",
  sitesFieldMemberName: "Name",
  sitesFieldBio: "Kurzporträt",
  sitesFieldQuestion: "Frage",
  sitesFieldAnswer: "Antwort",
  sitesFieldSuccessMessage: "Nachricht nach dem Absenden",
  sitesFieldFooterText: "Fußzeilentext",
  sitesContactFormHint:
    "Das Formular steht bereits auf der Seite; das Absenden funktioniert, sobald es die Formulare gibt.",
  sitesTheme: "Design",
  sitesThemeTitle: "Design der Website",
  sitesThemeSubtitle:
    "Fügen Sie Website-Logo und Browsersymbol hinzu. Markenfarben kommen aus Marke.",
  sitesThemeApply: "Design übernehmen",
  sitesThemeLoadFailed: "Die Design-Optionen konnten nicht geladen werden.",
  sitesThemePresets: "Farben & Schrift",
  sitesThemeBrandColors: "Markenakzente",
  sitesThemeBrandColorsHint:
    "Verwenden Sie den Primärakzent für wichtige Aktionen und den Sekundärakzent für unterstützende Hervorhebungen.",
  sitesThemeBrandManaged:
    "Diese Farben stammen aus Ihrem Marken-Kit. Ändern Sie sie einmal in Marke und verwenden Sie sie überall.",
  sitesThemeBaseColors: "Grundfarben",
  sitesThemeAccentColors: "Akzentfarben",
  sitesThemeResetColors: "Voreinstellungsfarben verwenden",
  sitesThemeBackgroundColor: "Hintergrund",
  sitesThemeTextColor: "Text",
  sitesThemeBorderColor: "Rahmen",
  sitesThemeAccentColor: (number: number) =>
    number === 1
      ? "Primärakzent"
      : number === 2
        ? "Sekundärakzent"
        : `Akzent ${number}`,
  sitesThemeHexValue: (label: string) => `HEX-Wert für ${label}`,
  sitesThemeColorError:
    "Verwenden Sie sechsstellige HEX-Farben und mindestens 4,5:1 Kontrast zwischen Text und Hintergrund.",
  sitesThemeLogo: "Logo",
  sitesThemeLogoHint:
    "Erscheint in der Navigationsleiste anstelle des Website-Namens.",
  sitesThemeFavicon: "Favicon",
  sitesThemeFaviconHint: "Das kleine Symbol, das Browser am Tab zeigen.",
  sitesThemeUpload: "Bild hochladen",
  sitesThemeReplace: "Bild ersetzen",
  sitesThemeRemove: "Bild entfernen",
  sitesThemeSet: "Bild hochgeladen",
  sitesThemeNotSet: "Noch keines",
  sitesUploadFailed: "Das Bild konnte nicht hochgeladen werden.",
  brandingTitle: "Marke",
  brandingSubtitle:
    "Erstellen Sie ein verlässliches Marken-Kit für Websites, Angebote, Rechnungen, Kampagnen, Dokumente und alle Markenausgaben.",
  brandingSave: "Marken-Kit speichern",
  brandingSaved: "Marken-Kit gespeichert",
  brandingUnsaved: "Ungespeicherte Änderungen",
  brandingAccentsTitle: "Markenakzente",
  brandingAccentsHint:
    "Dies sind gemeinsame Arbeitsbereichsrollen, nicht Farben für nur eine App. Primär trägt wichtige Aktionen und Wiedererkennung; Sekundär unterstützt, ohne zu konkurrieren.",
  brandingPrimary: "Primärakzent",
  brandingPrimaryHint:
    "Nutzen Sie diese Farbe für wichtigste Aktionen und Markenmomente, etwa Angebotsannahmen, Rechnungsakzente, Kampagnenaufrufe und Website-Schaltflächen.",
  brandingSecondary: "Sekundärakzent",
  brandingNeutral: "Neutraler Raum",
  brandingSecondaryHint:
    "Nutzen Sie diese ergänzende Farbe für sekundäre Aktionen, unterstützende Akzente, Diagramme und Kontrast. Sie soll Primär ergänzen, nicht mit ihm konkurrieren.",
  brandingAddSecondary: "Sekundärakzent hinzufügen",
  brandingRemoveSecondary: "Sekundärakzent entfernen",
  brandingSupportingTitle: "Unterstützende Farben",
  brandingSupportingHint:
    "Fügen Sie nur Farben mit klarem wiederverwendbarem Zweck hinzu, etwa Produktlinie, Kampagnenfamilie, Diagrammreihe oder Untermarke. Benennen Sie jede verständlich.",
  brandingAddSupporting: "Unterstützende Farbe hinzufügen",
  brandingSupportingLimit:
    "Bis zu drei unterstützende Farben halten die Palette konsistent und einfach.",
  brandingColorName: "Farbname",
  brandingColorHex: "HEX-Wert",
  brandingMoreInfo: (field: string) => `Weitere Informationen zu ${field}`,
  brandingSupportingName: (number: number) => `Unterstützend ${number}`,
  brandingRemoveColor: (name: string) => `${name} entfernen`,
  brandingMoveColorUp: (name: string) => `${name} nach oben verschieben`,
  brandingMoveColorDown: (name: string) => `${name} nach unten verschieben`,
  brandingInvalidColor:
    "Geben Sie jeder Farbe einen Namen und einen sechsstelligen HEX-Wert.",
  brandingPreviewTitle: "Live-Vorschau",
  brandingPreviewEyebrow: "Arbeitsbereichsmarke",
  brandingPreviewHeading: "Unverkennbar Ihre Marke",
  brandingPreviewBody:
    "Ihre Markenrollen fließen in Websites, Live-Angebote, Rechnungen, Kampagnen, Dokumente und alle benötigten Bereiche ein.",
  brandingPreviewPrimary: "Primäre Aktion",
  brandingPreviewSecondary: "Sekundäre Aktion",
  brandingVisualStudio: "Visuelles Markenstudio",
  brandingSeeItInUse: "Farben im echten Einsatz ansehen",
  brandingPreviewContexts: "Kontexte der Markenvorschau",
  brandingPreviewWebsite: "Website",
  brandingPreviewDocument: "Angebot & Rechnung",
  brandingPreviewCampaign: "Kampagne",
  brandingToneScale: "Primäre Farbskala",
  brandingGenerated: "Von Primär abgeleitet",
  brandingCopyColor: (color: string) => `${color} kopieren`,
  brandingColorCopied: (color: string) => `${color} kopiert`,
  quoteStudioImportBrandColors: "Markenfarben importieren",
  quoteStudioImportBrandTypography: "Markentypografie importieren",
  brandingToneScaleHint:
    "Nutzen Sie helle Töne für Hintergründe und dunkle für Akzente, ohne eine weitere Markenfarbe einzuführen.",
  brandingContrast: "Lesbarer Text",
  brandingUseLightText: "Hellen Text verwenden",
  brandingUseDarkText: "Dunklen Text verwenden",
  brandingColorBalance: "Farbbalance",
  brandingColorBalanceHint:
    "Lassen Sie neutralen Raum führen, nutzen Sie Sekundär für Struktur und Primär nur für wichtige Momente.",
  brandingGuidanceTitle: "Empfohlene Palette",
  brandingGuidancePrimary: "1 primär — erforderlich",
  brandingGuidanceSecondary: "1 sekundär — optional",
  brandingGuidanceSupporting: "0–2 unterstützend — üblich; bis zu 3 verfügbar",
  brandingWcagAa: "WCAG AA",
  brandingBalanceRatio: "70 / 20 / 10",
  brandingNavLabel: "Markenbereiche",
  brandingFoundationNav: "Markenfundament",
  brandingVisualIdentityNav: "Visuelle Identität",
  brandingApplicationsNav: "Markenanwendungen",
  brandingGuidelinesNav: "Richtlinien",
  brandingSaveFailed:
    "Das Marken-Kit konnte nicht gespeichert werden. Ihre Änderungen bleiben erhalten; versuchen Sie es erneut.",
  brandingFoundationTitle: "Definieren Sie, wofür Ihre Marke steht",
  brandingFoundationSubtitle:
    "Geben Sie jedem Team dieselbe klare Grundlage für Entscheidungen, Texte und Gestaltung.",
  brandingBrandName: "Markenname",
  brandingBrandNameHint:
    "Der öffentliche Name, der in allen Markenausgaben verwendet wird.",
  brandingBrandNamePlaceholder: "Markennamen eingeben",
  brandingTagline: "Claim",
  brandingTaglineHint:
    "Ein kurzes Versprechen oder eine Idee, die in Erinnerung bleiben soll.",
  brandingTaglinePlaceholder: "Ein prägnantes Markenversprechen formulieren",
  brandingPurpose: "Zweck",
  brandingPurposeHint:
    "Erklären Sie, warum die Marke über ihr Angebot hinaus existiert.",
  brandingPurposePlaceholder: "Warum gibt es Ihre Marke?",
  brandingAudience: "Zielgruppe",
  brandingAudienceHint:
    "Beschreiben Sie klar die Menschen oder Organisationen, für die Sie arbeiten.",
  brandingAudiencePlaceholder: "Für wen ist diese Marke?",
  brandingPositioning: "Positionierung",
  brandingPositioningHint:
    "Beschreiben Sie den Platz, den Ihre Marke im Markt einnehmen soll.",
  brandingPositioningPlaceholder:
    "Was unterscheidet Ihre Marke auf relevante Weise?",
  brandingPersonality: "Persönlichkeit",
  brandingPersonalityHint:
    "Wählen Sie einige menschliche Eigenschaften, die in jeder Interaktion erkennbar sein sollen.",
  brandingPersonalityPlaceholder: "Zum Beispiel: klar, warm, souverän",
  brandingVoice: "Sprache und Ton",
  brandingVoiceHint:
    "Beschreiben Sie, wie die Marke spricht und ihren Ton an den Kontext anpasst.",
  brandingVoicePlaceholder: "Wie soll die Marke klingen?",
  brandingVisualIdentityTitle:
    "Schaffen Sie eine wiedererkennbare visuelle Identität",
  brandingVisualIdentitySubtitle:
    "Legen Sie wiederverwendbare Elemente und Rollen fest, die Alo im gesamten Arbeitsbereich nutzt.",
  brandingLogoTitle: "Logo-Bibliothek",
  brandingLogoHint:
    "Verwalten Sie alle freigegebenen Logo-Varianten gemeinsam und wählen Sie die primäre Version für Alo.",
  brandingLogoDropTitle: "Logos hier ablegen oder durchsuchen",
  brandingLogoDropNow: "Ablegen, um diese Logos hinzuzufügen",
  brandingLogoPrimary: "Primär",
  brandingLogoMakePrimary: "Als primär festlegen",
  brandingLogoDisplayName: "Logo-Name",
  brandingLogoRemoveTitle: "Logo entfernen?",
  brandingLogoRemoveConfirm: (name: string) =>
    `${name} aus der Logo-Bibliothek entfernen? Dies kann nicht rückgängig gemacht werden.`,
  brandingLogoLimit: "Sie können bis zu 8 Logo-Varianten speichern.",
  brandingLogoCount: (count: number, maximum: number) =>
    `${count} von ${maximum}`,
  brandingLogoReplaceNamed: (name: string) => `${name} ersetzen`,
  brandingLogoRemoveNamed: (name: string) => `${name} entfernen`,
  brandingLogoUpload: "Logo hochladen",
  brandingLogoReplace: "Logo ersetzen",
  brandingLogoRemove: "Logo entfernen",
  brandingLogoRequirements: "SVG, PNG, JPEG oder WebP · maximal 500 KB",
  brandingLogoTooLarge: "Wählen Sie ein Logo unter 500 KB.",
  brandingLogoUnsupported:
    "Wählen Sie eine sichere SVG-, PNG-, JPEG- oder WebP-Datei.",
  brandingTypographyTitle: "Typografie",
  brandingTypographyHint:
    "Weisen Sie Überschriften und gut lesbaren Fließtexten jeweils eine Schriftrolle zu.",
  brandingHeadingFont: "Schrift für Überschriften",
  brandingBodyFont: "Schrift für Fließtext",
  brandingFontInter: "Inter",
  brandingFontArial: "Arial",
  brandingFontGeorgia: "Georgia",
  brandingFontGaramond: "Garamond",
  brandingColorsTitle: "Farbsystem",
  brandingColorsSubtitle:
    "Definieren Sie eine kleine, barrierearme Palette nach Rolle statt nach einzelner App.",
  brandingApplicationsTitle: "Sehen Sie die Marke in echten Anwendungen",
  brandingApplicationsSubtitle:
    "Prüfen Sie eine Identität in den Ausgaben, die Kunden und Teams tatsächlich verwenden.",
  brandingPreviewWorkspaceDocument: "Dokument",
  brandingGuidelinesTitle: "Ihre lebendigen Markenrichtlinien",
  brandingGuidelinesSubtitle:
    "Eine klare, druckbare Referenz aus demselben Marken-Kit, das jede Alo-App verwendet.",
  brandingPrintGuidelines: "Richtlinien drucken",
  brandingGuidelineFoundation: "Fundament",
  brandingGuidelineLogo: "Logo-Anwendung",
  brandingGuidelineColors: "Farbrollen",
  brandingGuidelineTypography: "Typografie",
  brandingGuidelineVoice: "Sprache und Ton",
  brandingGuidelineMissing: "Noch nicht definiert",
  brandingGuidelineLogoMissing: "Es wurde noch kein Hauptlogo hinzugefügt.",
  brandingGuidelineLogoRule:
    "Lassen Sie Freiraum um das Logo und verzerren oder färben Sie es nie um. Vermeiden Sie Hintergründe, die seine Lesbarkeit mindern.",
  brandingGuidelineColorRule:
    "Nutzen Sie die Primärfarbe für Wiedererkennung und wichtige Aktionen. Verwenden Sie unterstützende Farben nur für ihre benannte Rolle.",
  brandingGuidelineTypographyRule:
    "Nutzen Sie die Überschriftenschrift für Hierarchie und die Textschrift für längere, gut lesbare Inhalte.",
  brandingGuidelineVoiceRule:
    "Bewahren Sie dieselbe Persönlichkeit und passen Sie den Ton an Leser und Situation an.",
  brandingSampleName: "Atelier North",
  brandingSampleClient: "Northstar Studio",
  brandingSampleTagline: "Klarheit sichtbar gemacht",
  brandingPreviewWork: "Arbeiten",
  brandingPreviewAbout: "Über uns",
  brandingPreviewStartProject: "Projekt starten",
  brandingPreviewWebsiteEyebrow: "Unabhängiges Designstudio",
  brandingPreviewWebsiteHeading:
    "Ideen werden zu Marken, die Menschen im Gedächtnis bleiben.",
  brandingPreviewWebsiteBody:
    "Eine klare Identität, durchdachte digitale Erlebnisse und ein System, das Ihr Team sicher anwenden kann.",
  brandingPreviewExploreWork: "Arbeiten ansehen",
  brandingPreviewOurApproach: "Unser Ansatz",
  brandingPreviewLaunches: "Markteinführungen",
  brandingPreviewCountries: "Länder",
  brandingPreviewReferred: "empfohlen",
  brandingPreviewQuoteLabel: "ANGEBOT",
  brandingPreviewPreparedFor: "Erstellt für",
  brandingPreviewQuote: "Angebot",
  brandingPreviewBrandStrategy: "Markenstrategie",
  brandingPreviewVisualIdentity: "Visuelle Identität",
  brandingPreviewLaunchToolkit: "Launch-Toolkit",
  brandingPreviewTotal: "Gesamt",
  brandingPreviewQuoteFooter:
    "Vielen Dank für die Gelegenheit, gemeinsam etwas Unvergessliches zu schaffen.",
  brandingPreviewCampaignBadge: "NEUE KOLLEKTION",
  brandingPreviewCampaignEyebrow: "Ein durchdachtes neues Kapitel",
  brandingPreviewCampaignHeading: "Entwickelt für Ihre heutige Arbeitsweise.",
  brandingPreviewCampaignBody:
    "Entdecken Sie eine Kollektion, die auf Klarheit, Handwerk und dauerhaftem Nutzen beruht.",
  brandingPreviewCampaignAction: "Kollektion ansehen",
  brandingPreviewCampaignLocation: "Brüssel",
  brandingPreviewDocumentType: "PROJEKTBRIEFING",
  brandingPreviewDocumentHeading: "Ein klarerer Weg von der Idee bis zum Start",
  brandingPreviewDocumentBody:
    "Dieses Dokument vereint das Team hinter einem Zweck, einer Zielgruppe und einer sicheren Richtung.",
  brandingPreviewDocumentSection: "Die Chance",
  brandingPreviewDocumentSectionBody:
    "Machen Sie aus einer starken Haltung ein konsistentes Erlebnis an jedem Kundenkontaktpunkt.",
  sitesUploadImage: "Bild hochladen",
  sitesPublish: "Veröffentlichen",
  sitesPublishChanges: "Änderungen veröffentlichen",
  sitesUnpublish: "Vom Netz nehmen",
  sitesConfirmUnpublish: "Wirklich vom Netz nehmen?",
  sitesLiveAtLabel: "Ihre Website ist online unter",
  sitesGoesLiveAt: (address: string) =>
    `Mit dem Veröffentlichen geht diese Website online unter ${address}.`,
  sitesAddressPreview: (address: string) =>
    `Ihre Website wird unter ${address} erreichbar sein.`,
  sitesPublishFailed: "Die Website konnte nicht veröffentlicht werden.",
  sitesUnpublishFailed: "Die Website konnte nicht vom Netz genommen werden.",
  sitesSubmissions: "Nachrichten",
  sitesSubmissionsLoadFailed:
    "Ihre Formularnachrichten konnten nicht geladen werden.",
  sitesSubmissionSaveFailed:
    "Diese Nachricht konnte nicht aktualisiert werden.",
  sitesNoSubmissionsTitle: "Noch keine Nachrichten",
  sitesNoSubmissionsBody:
    "Setzen Sie ein Kontaktformular auf eine Seite. Neue Nachrichten von Besuchern erscheinen hier.",
  sitesOpenPages: "Seiten öffnen",
  sitesSubmissionList: "Nachrichten von Besuchern",
  sitesSubmissionDetail: "Ausgewählte Nachricht",
  sitesHandled: "Erledigt",
  sitesNeedsReply: "Braucht Antwort",
  sitesMarkHandled: "Als erledigt markieren",
  sitesReopenSubmission: "Wieder öffnen",
  sitesForm: "Formular",
  sitesReceived: "Eingegangen",
  sitesExportSubmissions: "Als CSV exportieren",
  sitesExportingSubmissions: "Export wird vorbereitet…",
  sitesSubmissionsExportFailed:
    "Ihre Nachrichten konnten nicht exportiert werden. Versuchen Sie es erneut.",
  sitesAssistant: "Assistent",
  sitesAssistantTitle: "Website-Assistent",
  sitesAssistantLoadFailed:
    "Die Einstellungen des Assistenten konnten nicht geladen werden. Versuchen Sie es erneut.",
  sitesAssistantSwitchTitle: "Der Assistent und sein Budget",
  sitesAssistantSwitchHint:
    "Ein Chat-Assistent auf Ihrer veröffentlichten Website, der Besucherfragen aus Ihren veröffentlichten Seiten beantwortet — und immer die Seite nennt, aus der eine Antwort stammt.",
  sitesAssistantEnable:
    "Besucherfragen auf der veröffentlichten Website beantworten",
  sitesAssistantBudgetLabel: "Monatsbudget (€)",
  sitesAssistantBudgetHint: (defaultBudget: string) =>
    `Antworten kosten Geld. Erreichen die Antworten eines Monats dieses Budget, pausiert der Assistent, und Besucher werden stattdessen auf Ihr Kontaktformular verwiesen — Sie werden benachrichtigt. Lassen Sie das Feld leer, gilt ${defaultBudget}.`,
  sitesAssistantBudgetNotANumber:
    "Geben Sie das Monatsbudget als Zahl in Euro ein.",
  sitesAssistantSpent: (spent: string, budget: string) =>
    `${spent} von ${budget} in diesem Monat ausgegeben.`,
  sitesAssistantCeilingHit:
    "Das Budget dieses Monats ist aufgebraucht; der Assistent pausiert, und Besuchern wird Ihr Kontaktformular angeboten. Ein höheres Budget öffnet ihn sofort wieder.",
  sitesAssistantSave: "Speichern",
  sitesAssistantSaved: "Gespeichert.",
  sitesAssistantSaveFailed:
    "Die Einstellungen des Assistenten konnten nicht gespeichert werden. Versuchen Sie es erneut.",
  sitesAssistantReadsTitle: "Was der Assistent liest",
  sitesAssistantReadsRule:
    "Was der Assistent lesen kann, kann jeder im Internet lesen — er beantwortet damit die Fragen Fremder.",
  sitesAssistantReadsPublishedSite:
    "Ihre veröffentlichte Website — jede Seite, die online ist",
  sitesAssistantReadsPublishedPosts: "Ihre veröffentlichten Blogartikel",
  sitesAssistantAlwaysRead: "wird immer gelesen",
  sitesAssistantNoKnowledge:
    "Noch keine Dokumente für den Assistenten veröffentlicht. Er antwortet allein aus Ihrer veröffentlichten Website.",
  sitesAssistantAddedOn: (date: string) => `veröffentlicht am ${date}`,
  sitesAssistantTrashed: "im Drive-Papierkorb — wird nicht mehr gelesen",
  sitesAssistantWithdraw: (title: string) => `${title} zurückziehen`,
  sitesAssistantWithdrawFailed:
    "Das Dokument konnte nicht vom Assistenten zurückgezogen werden. Versuchen Sie es erneut.",
  sitesAssistantInternetWarning: "Jeder im Internet wird das lesen können.",
  sitesAssistantPublishDocument:
    "Ein Dokument für den Assistenten veröffentlichen…",
  sitesAssistantPublishFailed:
    "Das Dokument konnte nicht für den Assistenten veröffentlicht werden. Versuchen Sie es erneut.",
  sitesAssistantPickerTitle: "Ein Dokument für den Assistenten veröffentlichen",
  sitesAssistantPickerSubtitle:
    "Wählen Sie ein lesbares Dokument — der Assistent beantwortet Besucherfragen daraus.",
  sitesAssistantPickerConfirm: "Für den Assistenten veröffentlichen",
  sitesAssistantPickerBack: "Zurück zum übergeordneten Ordner",
  sitesAssistantPickerSearch: "In diesem Ordner suchen",
  sitesAssistantPickerEmpty: "Nichts in diesem Ordner.",
  sitesAssistantDidTitle: "Was der Assistent getan hat",
  sitesAssistantDidHint:
    "Jede Handlung des Assistenten in Ihrem Namen, mit dem verwendeten Fakt und der Seite, von der er stammt. Was Besucher getippt haben, wird nie gespeichert.",
  sitesAssistantDidEmpty:
    "Noch nichts. Beantwortet der Assistent eine Frage, bietet freie Zeiten an, bucht einen Termin oder speichert einen Lead, erscheint jede Handlung hier.",
  sitesAssistantDidLoadFailed:
    "Was der Assistent getan hat, konnte nicht geladen werden. Versuchen Sie es erneut.",
  sitesAssistantDidAnswered: "Hat eine Frage beantwortet",
  sitesAssistantDidAnsweredUsing: (pages: string) =>
    `Hat eine Frage beantwortet — gestützt auf ${pages}`,
  sitesAssistantDidRefused:
    "Hat eine Frage abgelehnt, die sich aus Ihren veröffentlichten Seiten nicht beantworten ließ",
  sitesAssistantDidBookingOffered: (service: string) =>
    `Hat freie Zeiten für „${service}“ angeboten`,
  sitesAssistantDidBooked: (service: string, when: string) =>
    `Hat „${service}“ für ${when} gebucht — der Termin steht in Ihrem Kalender`,
  sitesAssistantDidLeadOffered: "Hat im Gespräch das Kontaktformular angeboten",
  sitesAssistantDidLeadSaved:
    "Hat einen neuen Lead auf Ihrem CRM-Board gespeichert",
  sitesAssistantDidLeadKnown:
    "Hat einem wiederkehrenden Kontakt gesagt, dass Sie ihn schon kennen — kein Duplikat wurde angelegt",
  sitesAssistantDidTicketsOffered: (event: string) =>
    `Hat Tickets für „${event}“ angeboten, zum Preis aus der eigenen Preisliste`,
  sitesAssistantLookTitle: "Wie er aussieht und spricht",
  sitesAssistantLookHint:
    "Das Widget trägt bereits Design, Logo und Sprache Ihrer Website. Was Sie hier wählen, sind seine Worte und ein paar begrenzte Entscheidungen — Farbe bleibt innerhalb der eigenen Palette Ihrer Website.",
  sitesAssistantBotNameLabel: "Name des Assistenten",
  sitesAssistantBotNameHint:
    "Oft bewusst nicht der Firmenname — „Fragen Sie Marie“ schlägt „Chatten Sie mit uns“.",
  sitesAssistantAvatarLabel: "Avatar",
  sitesAssistantAvatarHint:
    "Ein kleines Foto im Kopf des Widgets. Ein Gesicht wirkt besser als ein Logo.",
  sitesAssistantWelcomeLabel: "Begrüßung",
  sitesAssistantWelcomeDefaultNote:
    "Das ist die mitgelieferte Vorgabe, in der Sprache Ihrer Website — behalten Sie sie oder machen Sie sie zu Ihrer.",
  sitesAssistantQuestionsLegend: "Vorgeschlagene Fragen",
  sitesAssistantQuestionsHint:
    "Bis zu drei Fragen zum Antippen, angeboten, bis Besucher ihre eigene stellen.",
  sitesAssistantQuestionLabel: (n: number) => `Vorgeschlagene Frage ${n}`,
  sitesAssistantSuggestFromSite: "Aus Ihrer Website vorschlagen",
  sitesAssistantSuggestedApplied:
    "Entworfen aus den eigenen Seiten Ihrer Website — bearbeiten Sie sie frei.",
  sitesAssistantSuggestedNone:
    "Noch nichts da, woraus sich etwas entwerfen ließe. Ein FAQ-, Preis-, Buchungs- oder Kontaktabschnitt auf Ihren Seiten gibt dem hier etwas an die Hand.",
  sitesAssistantSuggestFailed:
    "Ihre Seiten konnten nicht für Vorschläge gelesen werden. Versuchen Sie es erneut.",
  sitesAssistantSuggestedPricing: "Was kostet es?",
  sitesAssistantSuggestedBooking: "Kann ich einen Termin buchen?",
  sitesAssistantSuggestedCatalog: "Was bieten Sie an?",
  sitesAssistantSuggestedContact: "Wie erreiche ich Sie?",
  sitesAssistantAppearanceSave: "Erscheinungsbild speichern",
  sitesAssistantToneLegend: "Ton",
  sitesAssistantToneFormal: "Förmlich",
  sitesAssistantToneNeutral: "Neutral",
  sitesAssistantToneWarm: "Warm",
  sitesAssistantToneNoteLabel: "Notiz zum Tonfall",
  sitesAssistantToneNoteHint:
    "Wie Ihr Geschäft spricht — einfache Worte, kein Fachjargon, so etwas. Nur Stil: Was der Assistent sagen oder versprechen darf, kann das nie ändern.",
  sitesAssistantCornerLegend: "Ecke des Startknopfs",
  sitesAssistantCornerRight: "Unten rechts",
  sitesAssistantCornerLeft: "Unten links",
  sitesAssistantIconLegend: "Symbol des Startknopfs",
  sitesAssistantIconChat: "Sprechblase",
  sitesAssistantIconQuestion: "Fragezeichen",
  sitesAssistantIconSparkle: "Funkeln",
  sitesAssistantAccentLegend: "Farbe",
  sitesAssistantAccentHint:
    "Eine Wahl unter den Palettenrollen Ihrer eigenen Website — jede Möglichkeit bleibt gut lesbar.",
  sitesAssistantAccentPrimary: "Markenfarbe",
  sitesAssistantAccentText: "Tinte",
  sitesAssistantAccentSurface: "Dezent",
  sitesAssistantAutoOpenLabel: "Beim Laden der Seite von selbst öffnen",
  sitesAssistantAutoOpenHint:
    "Standardmäßig aus — ein ungebetenes Popup ist das, was alle hassen. Eingeschaltet öffnet es sich, ohne die Tastatur an sich zu reißen.",
  sitesAssistantOfflineLabel: "Offline-Nachricht",
  sitesAssistantOfflineHint:
    "Zu sehen, wenn der Assistent nicht antworten kann — das Monatsbudget ist aufgebraucht, oder keine KI ist eingerichtet.",
  sitesAssistantPreviewTitle: "Vorschau",
  sitesAssistantPreviewHint:
    "Das echte Widget im Design Ihrer Website, geöffnet gezeigt. Besucher sehen es zuerst geschlossen in seiner Ecke.",
  sitesAssistantPreviewFrameTitle: "Vorschau des Assistenten-Widgets",
  sitesAssistantPreviewFailed: "Die Vorschau konnte nicht dargestellt werden.",
  sitesAssistantA11yTitle: "Barrierefreiheit",
  sitesAssistantA11yContrast: (ratio: string) =>
    `Text auf der gewählten Farbe misst ${ratio}:1 — über der WCAG-AA-Marke von 4,5:1.`,
  sitesAssistantA11yContrastGuarantee:
    "Jede Farbwahl hier wird auf dem Server gegen Ihre Palette auf Kontrast geprüft — keine Möglichkeit kann eine unlesbare Kombination speichern.",
  sitesAssistantA11yKeyboard:
    "Das Widget ist ein beschrifteter Dialog: durchgehend mit der Tastatur bedienbar, Escape schließt es, und Antworten werden von Screenreadern angesagt, sobald sie eintreffen.",
  sitesAssistantA11yAvatar:
    "Der Avatar ist dekorativ und vor Screenreadern verborgen — angesagt wird der Name des Assistenten.",
  sitesAnalytics: "Statistiken",
  sitesAnalyticsLoadFailed:
    "Ihre Website-Statistiken konnten nicht geladen werden. Versuchen Sie es erneut.",
  sitesAnalyticsLoading: "Website-Statistiken werden geladen",
  sitesAnalyticsPeriod: "Zeitraum der Statistiken",
  sitesAnalyticsDays: (days: number) => `${days} Tage`,
  sitesAnalyticsSummary: "Besuchsüberblick",
  sitesAnalyticsVisits: "Besuche",
  sitesAnalyticsVisitors: "Besucher pro Tag",
  sitesAnalyticsOverTime: "Besuche im Zeitverlauf",
  sitesAnalyticsChartLabel: "Tägliche Website-Besuche",
  sitesAnalyticsDayLabel: (date: string, visits: number) =>
    `${date}: ${visits} ${visits === 1 ? "Besuch" : "Besuche"}`,
  sitesAnalyticsTopPages: "Meistbesuchte Seiten",
  sitesAnalyticsTopReferrers: "Häufigste Verweise",
  sitesAnalyticsDirect: "Direkt",
  sitesAnalyticsPrivacyTitle: "Keine Cookies. Kein Banner.",
  sitesAnalyticsPrivacyBody:
    "Besuche werden anonym pro Tag gezählt. alo speichert keine Besucheradresse, kein Geräteprofil und keinen Browserverlauf.",
  sitesAnalyticsPrivacyBeacon:
    "Lesezeit und ausgehende Klicks meldet ein kleines Skript auf Ihren Seiten. Es trägt keinerlei Identität, daher lassen sich zwei Meldungen aus demselben Browser nicht verknüpfen.",
  sitesAnalyticsEmptyTitle: "Noch keine Besuche",
  sitesAnalyticsEmptyBody:
    "Öffnen oder teilen Sie Ihre veröffentlichte Website. Ihre ersten Besuche erscheinen hier von selbst.",
  sitesAnalyticsOpenSite: "Website online öffnen",
  sitesAnalyticsGroupArrival: "Wie Sie gefunden wurden",
  sitesAnalyticsGroupPages: "Was angesehen wurde",
  sitesAnalyticsGroupReading: "Wie es gelesen wurde",
  sitesAnalyticsShowAll: (count: number) => `Alle ${count} zeigen`,
  sitesAnalyticsShowTop: (count: number) => `Nur die ersten ${count} zeigen`,
  sitesAnalyticsReferrersNote:
    "Die Website, von der aus ein Besucher einem Link folgte. Nur die Domain wird behalten, nie die Seite.",
  sitesAnalyticsReferrersEmpty:
    "Noch keine Verweise. Sie erscheinen, wenn eine andere Website auf Ihre verlinkt.",
  sitesAnalyticsCampaigns: "Kampagnen",
  sitesAnalyticsCampaignsNote:
    "Gelesen aus utm_campaign an den Links, die Sie teilen — so unterscheiden Sie einen Newsletter von einem Plakat.",
  sitesAnalyticsCampaignsEmpty:
    "Noch keine Kampagnen. Hängen Sie ?utm_campaign=fruehjahrs-mailing an einen geteilten Link, und seine Besuche werden hier gezählt.",
  sitesAnalyticsNoCampaign: "Ohne Kampagne",
  sitesAnalyticsCountries: "Länder",
  sitesAnalyticsCountriesNote:
    "Bestimmt vom Netz vor Ihrer Website, nie aus einer gespeicherten Besucheradresse.",
  sitesAnalyticsCountriesEmpty:
    "Keine Länder gemeldet. Ihre Website wird ohne ein Netz ausgeliefert, das sie benennt — dieses Feld bleibt leer, und jede andere Zahl hier bleibt unberührt.",
  sitesAnalyticsNotReported: "Nicht gemeldet",
  sitesAnalyticsTopPagesNote: "Die Seiten, die am häufigsten geöffnet wurden.",
  sitesAnalyticsPagesEmpty: "In diesem Zeitraum noch keine Seiten gezählt.",
  sitesAnalyticsEntryPages: "Erste Seiten",
  sitesAnalyticsEntryPagesNote:
    "Die Seite, auf der der Tag eines Besuchers auf Ihrer Website begann.",
  sitesAnalyticsExitPages: "Letzte Seiten",
  sitesAnalyticsExitPagesNote:
    "Die letzte an dem Tag gesehene Seite. Eine letzte Seite ist, wo jemand zu Ende gelesen hat — nicht unbedingt, wo jemand aufgab.",
  sitesAnalyticsReadTime: "Lesezeit",
  sitesAnalyticsReadTimeNote:
    "Wie lange Seiten auf dem Bildschirm blieben, für die ganze Website statt je Seite. Gezählt werden nur Browser, die es melden — deshalb ergeben diese Zahlen in Summe nie Ihre Besuche.",
  sitesAnalyticsReadTimeEmpty:
    "Noch keine Lesezeiten. Sie kommen, sobald Besucher Ihre veröffentlichten Seiten in einem Browser öffnen, der sie meldet.",
  sitesAnalyticsReadUnder10s: "Unter 10 Sekunden",
  sitesAnalyticsRead10to30s: "10–30 Sekunden",
  sitesAnalyticsRead30to60s: "30–60 Sekunden",
  sitesAnalyticsRead1to3m: "1–3 Minuten",
  sitesAnalyticsRead3to10m: "3–10 Minuten",
  sitesAnalyticsReadOver10m: "Über 10 Minuten",
  sitesAnalyticsOutbound: "Links nach draußen",
  sitesAnalyticsOutboundNote:
    "Domains, zu denen Besucher weitergezogen sind. Ab 200 Zielen an einem Tag wird der Rest zusammen gezählt.",
  sitesAnalyticsOutboundEmpty:
    "Noch keine ausgehenden Klicks. Gezählt wird, wenn ein Besucher einem Link auf eine andere Website folgt.",
  sitesAnalyticsOutboundOther: "Weitere Domains",
  sitesAnalyticsDevices: "Geräte",
  sitesAnalyticsDevicesNote:
    "Eine grobe Klasse aus dem, was der Browser über sich selbst sagt. Mehr davon wird nicht gespeichert.",
  sitesAnalyticsDevicesEmpty: "In diesem Zeitraum noch keine Geräte gezählt.",
  sitesAnalyticsDevicePhone: "Telefon",
  sitesAnalyticsDeviceTablet: "Tablet",
  sitesAnalyticsDeviceDesktop: "Computer",
  sitesAnalyticsDeviceBot: "Bots und Crawler",
  sitesAnalyticsDeviceUnknown: "Nicht erkannt",
  sitesHeatmap: "Aufmerksamkeitskarte",
  sitesBackToAnalytics: "Zurück zu den Statistiken",
  sitesHeatmapLoadFailed:
    "Die Aufmerksamkeitskarte konnte nicht geladen werden. Versuchen Sie es erneut.",
  sitesHeatmapLoading: "Aufmerksamkeitskarte wird geladen",
  sitesHeatmapPage: "Seite",
  sitesHeatmapPageOption: (path: string, events: number) =>
    `${path} — ${events} gezählt`,
  sitesHeatmapScreens: "Bildschirmgröße",
  sitesHeatmapScreenTab: (screen: string, events: string) =>
    `${screen} (${events})`,
  sitesHeatmapPrivacyTitle: "Eine Form, keine Aufzeichnung.",
  sitesHeatmapPrivacyBody:
    "Klicks und Lesetiefe werden je Bereich der Seite gezählt, pro Tag. Es gibt keine Cursorspur, kein Session-Replay und nichts, was zwei Besuche derselben Person verbinden könnte.",
  sitesHeatmapPrivacyShape:
    "Gezählt werden nur Browser, die es melden, und höchstens zwanzig Klicks je Seitenaufruf. Lesen Sie das als: wohin die Aufmerksamkeit ging — nie als: wie viele Menschen etwas taten.",
  sitesHeatmapEmptyTitle: "Noch nichts zu zeichnen",
  sitesHeatmapEmptyBody:
    "Klicks und Lesetiefe erscheinen hier, sobald Besucher Ihre veröffentlichten Seiten öffnen. Nichts muss eingeschaltet werden.",
  sitesHeatmapClicks: "Wo geklickt wurde",
  sitesHeatmapClicksNote:
    "Die ganze Seite von oben bis unten, nicht ein Bildschirm voll. Ein dunkleres Feld ist ein Bereich, der öfter geklickt wurde.",
  sitesHeatmapClicksLabel: (path: string, screen: string, clicks: number) =>
    `Karte der Stellen, an denen ${clicks} Klicks auf ${path} landeten — Bildschirmgröße ${screen}`,
  sitesHeatmapTop: "Anfang der Seite",
  sitesHeatmapBottom: "Ende der Seite",
  sitesHeatmapLegendQuiet: "Ruhiger",
  sitesHeatmapLegendBusy: "Belebter",
  sitesHeatmapLeft: "Links",
  sitesHeatmapCentre: "Mitte",
  sitesHeatmapRight: "Rechts",
  sitesHeatmapSpot: (side: string, band: string) => `${side}, ${band}`,
  sitesHeatmapDepthBand: (from: number, to: number) =>
    `${from}–${to} % der Seitenhöhe`,
  sitesHeatmapSpots: "Belebteste Bereiche",
  sitesHeatmapSpotsNote:
    "Dieselbe Karte in Worten, damit sie sich ohne die Farben lesen lässt.",
  sitesHeatmapClicksEmpty:
    "Auf dieser Seite wurde in dieser Bildschirmgröße nichts angeklickt.",
  sitesHeatmapSpotsEmpty: "Noch nichts zu beschreiben.",
  sitesHeatmapSpotsHeldBack:
    "Zurückgehalten, bis genug Klicks gezählt sind, um etwas zu beschreiben.",
  sitesHeatmapDepth: "Wie weit gelesen wurde",
  sitesHeatmapDepthNote:
    "Wie viele Lesende jedes Zehntel der Seite erreichten. Gezählt werden nur Browser, die es melden — in Summe ergibt das nie Ihre Besuche.",
  sitesHeatmapDepthEmpty:
    "In dieser Bildschirmgröße wurde hier keine Lesetiefe gezählt.",
  sitesHeatmapTooFewTitle: "Zu wenig für eine Karte",
  sitesHeatmapTooFewClicks: (collected: number, needed: number) =>
    `${collected} von ${needed} Klicks in dieser Bildschirmgröße gezählt. Eine Karte aus einer Handvoll Klicks zeigt die Handvoll, nicht Ihre Besucher — darum bleibt sie zurückgehalten, bis es genug sind.`,
  sitesHeatmapTooFewDepth: (collected: number, needed: number) =>
    `${collected} von ${needed} Lesemeldungen in dieser Bildschirmgröße gezählt. Die Kurve erscheint, sobald genug da sind, dass sie etwas bedeutet.`,
  sitesFunnel: "Ergebnisse",
  sitesFunnelPeriod: "Zeitraum",
  sitesFunnelLoading: "Ergebnisse werden geladen",
  sitesFunnelLoadFailed:
    "Die Ergebnisse konnten nicht geladen werden. Versuchen Sie es erneut.",
  sitesFunnelDeniedTitle: "Nicht Teil Ihres Zugangs",
  sitesFunnelDeniedFallback:
    "Diese Seite liest alo CRM und alo Billing, die für dieses Konto nicht freigeschaltet sind.",
  sitesFunnelDeniedWay:
    "Alles andere an dieser Website — ihre Seiten, ihre Anfragen, ihre Besuche — bleibt Ihnen zum Arbeiten offen.",
  sitesFunnelNoSourcesTitle: "Noch kein Kontaktformular",
  sitesFunnelNoSourcesBody:
    "Setzen Sie ein Kontaktformular auf eine Seite, und jede Anfrage daraus lässt sich vom ersten Seitenaufruf bis zur Rechnung verfolgen.",
  sitesFunnelChain: "Vom Besuch zur Rechnung",
  sitesFunnelStageViews: "Formular gesehen",
  sitesFunnelStageStarts: "Zu tippen begonnen",
  sitesFunnelStageSubmits: "Anfragen",
  sitesFunnelStageLeads: "An den Vertrieb übergeben",
  sitesFunnelStageWon: "Gewonnen",
  sitesFunnelStageInvoices: "Rechnungen",
  sitesFunnelFromBrowser: "Vom Browser gemeldet",
  sitesFunnelFromRecord: "Beim Speichern gezählt",
  sitesFunnelFloorNote:
    "Die ersten beiden Schritte meldet der Browser der Besucher, und ein Browser, der nichts meldet, hat die Seite trotzdem gesehen. Alles ab der Anfrage wird gezählt, als der Datensatz geschrieben wurde. Lesen Sie diese Zahlen als Untergrenze: Eine Quote über diese Linie hinweg ist die kleinstmögliche, keine Messung.",
  sitesFunnelMoney: "Das Geld dahinter",
  sitesFunnelInvoiceRule:
    "Rechnungen an den Kunden, der aus einer Anfrage wurde, gestellt nach der Übergabe.",
  sitesFunnelMoneyEmpty: "Aus dieser Website ist noch kein Deal entstanden.",
  sitesFunnelOpen: "In Arbeit",
  sitesFunnelWon: "Gewonnen",
  sitesFunnelInvoiced: "In Rechnung gestellt",
  sitesFunnelHidden: "Nicht gezeigt",
  sitesFunnelBillingOff:
    "Rechnungszahlen werden nicht gezeigt, weil alo Billing für dieses Konto nicht freigeschaltet ist. Das ist nicht dasselbe wie: Es wurde nichts in Rechnung gestellt.",
  sitesFunnelCurrencies:
    "Zwei Währungen sind zwei Zeilen und keine Summe: Eine Prognose hat kein Ausstellungsdatum, zu dem sich umrechnen ließe.",
  sitesFunnelSources: "Je Kontaktformular",
  sitesFunnelColSource: "Kontaktformular",
  sitesFunnelColDeals: "Deals",
  sitesFunnelDealsSummary: (open: number, won: number, lost: number) =>
    `${open} offen · ${won} gewonnen · ${lost} verloren`,
  sitesFunnelSumNote:
    "Eine Rechnung, die von zwei Formularen aus erreichbar ist, zählt einmal für die Website und einmal unter jedem Formular — diese Spalten sind eine Lesart je Formular und addieren sich nicht zu den Summen darüber.",
  sitesFunnelDeletedSource: "Gelöschtes Formular",
  sitesFunnelChatSource: "Website-Assistent",
  sitesHandoffSection: "Vertrieb",
  sitesHandoffInvite:
    "Machen Sie aus dieser Anfrage einen Deal auf Ihrem Vertriebsboard. Nichts auf diesem Bildschirm muss noch einmal getippt werden.",
  sitesHandoffTitle: "Diese Anfrage an den Vertrieb geben",
  sitesHandoffSubtitle:
    "Legt einen Deal auf Ihrem Vertriebsboard an und verknüpft ihn mit dieser Anfrage.",
  sitesHandoffSubmit: "An den Vertrieb geben",
  sitesHandoffFrom: "Von",
  sitesHandoffCarried:
    "Name, Adresse und Nachricht reisen mit der Übergabe — Sie tippen sie nie neu.",
  sitesHandoffTitleFor: (who: string) => `Website-Anfrage — ${who}`,
  sitesHandoffBoard: "Board",
  sitesHandoffColumn: "Spalte",
  sitesHandoffCardTitle: "Deal",
  sitesHandoffValue: "Erwarteter Wert",
  sitesHandoffValueHint:
    "Optional — was er Ihrer Einschätzung nach wert sein könnte.",
  sitesHandoffCurrency: "Währung",
  sitesHandoffCurrencyHint:
    "Leer lassen für die Währung Ihres Arbeitsbereichs.",
  sitesHandoffLoadingBoards: "Ihre Vertriebsboards werden geladen…",
  sitesHandoffNoBoards:
    "Es gibt noch kein Vertriebsboard, an das sich das geben ließe. Öffnen Sie alo CRM einmal, und Ihr erstes Board wird für Sie angelegt.",
  sitesHandoffCrmDenied: "alo CRM ist für dieses Konto nicht freigeschaltet.",
  sitesHandoffBoardsFailed:
    "Ihre Vertriebsboards konnten nicht geladen werden. Versuchen Sie es erneut.",
  sitesHandoffFailed:
    "Diese Anfrage konnte nicht übergeben werden. Versuchen Sie es erneut.",
  sitesInSales: "Im Vertrieb",
  sitesLeadsLoadFailed:
    "Die Vertriebsverknüpfungen dieses Posteingangs konnten nicht geladen werden.",
  sitesLeadStanding: (state: string, value: string) => `${state} · ${value}`,
  sitesLeadOpen: "In Arbeit",
  sitesLeadWon: "Gewonnen",
  sitesLeadLost: "Verloren",
  sitesUnlinkLead: "Verknüpfung lösen",
  sitesUnlinkLeadFailed:
    "Die Verknüpfung konnte nicht gelöst werden. Der Deal selbst ist unberührt. Versuchen Sie es erneut.",
  sitesHistory: "Versionsverlauf",
  sitesHistorySubtitle:
    "Jede Fassung dieser Website, die Sie veröffentlicht haben. Sehen Sie sich jede an, und stellen Sie eine mit einem Klick wieder online.",
  sitesHistoryLoadFailed: "Der Versionsverlauf konnte nicht geladen werden.",
  sitesHistoryVersions: "Veröffentlichte Fassungen",
  sitesHistoryLiveNow: "Jetzt online",
  sitesHistoryVersionOf: (date: string) => `Fassung vom ${date}`,
  sitesHistoryPagesCount: (pages: number) =>
    `${pages} ${pages === 1 ? "Seite" : "Seiten"}`,
  sitesHistoryLanguages: (languages: string) => `Sprachen: ${languages}`,
  sitesHistoryRestoredCopy: (date: string) =>
    `Eine Kopie der Fassung vom ${date}`,
  sitesHistoryRestore: "Diese Fassung wieder online stellen",
  sitesHistoryRestoring: "Wird wieder online gestellt…",
  sitesHistoryRestoreFailed:
    "Diese Fassung konnte nicht wieder online gestellt werden.",
  sitesHistoryRestored: (date: string) =>
    `Die Fassung vom ${date} ist wieder online.`,
  sitesHistoryUndo: "Rückgängig",
  sitesHistoryUndone: (date: string) =>
    `Zurück zur Fassung vom ${date}. Nichts ging verloren — jede Fassung ist noch da.`,
  sitesHistoryPage: "Seite",
  sitesHistoryPreviewLoadFailed: "Diese Fassung konnte nicht angezeigt werden.",
  sitesHistoryPreviewLoading: "Diese Fassung wird geladen",
  sitesHistoryPreviewTitle: "Vorschau der veröffentlichten Fassung",
  sitesHistoryDraftSafe:
    "Ihre laufende Arbeit bleibt unberührt: Eine Fassung wieder online zu stellen ändert nie, woran Sie gerade bauen.",
  sitesHistoryIfRestored: "Wenn Sie diese Fassung wieder online stellen",
  sitesHistoryIdentical: "Das ist genau, was jetzt online ist.",
  sitesHistoryThemeChange: "Das Aussehen der Website würde sich ändern.",
  sitesHistoryLanguagesBack: (languages: string) =>
    `Diese Sprachen kämen zurück: ${languages}`,
  sitesHistoryLanguagesGone: (languages: string) =>
    `Diese Sprachen fielen weg: ${languages}`,
  sitesHistoryPageBack: (page: string) => `${page} käme zurück`,
  sitesHistoryPageGone: (page: string) => `${page} fiele weg`,
  sitesHistoryPageChanged: (page: string) => `${page} würde sich ändern`,
  sitesHistoryUnchangedPages: (pages: number) =>
    pages === 1 ? "1 Seite bleibt gleich" : `${pages} Seiten bleiben gleich`,
  sitesHistoryEmptyTitle: "Noch nichts veröffentlicht",
  sitesHistoryEmptyBody:
    "Veröffentlichen Sie diese Website einmal, und jede veröffentlichte Fassung bleibt hier — zum Zurückblicken, und um sie wieder online zu stellen.",
  sitesScheduleTitle: "Zu einem gewählten Zeitpunkt veröffentlichen",
  sitesScheduleHint:
    "Wählen Sie Datum und Uhrzeit, und diese Website geht von selbst online. Sie müssen nicht dabei sein, wenn es geschieht.",
  sitesScheduleLoading: "Es wird geprüft, was geplant ist",
  sitesScheduleLoadFailed:
    "Die geplante Veröffentlichung konnte nicht geladen werden.",
  sitesScheduleOpen: "Veröffentlichung planen",
  sitesScheduleChange: "Den Zeitpunkt ändern",
  sitesScheduleWhen: "Datum und Uhrzeit",
  sitesScheduleGoesLive: (moment: string) => `Geht online am ${moment}.`,
  sitesScheduleTimeZone: (zone: string) =>
    `Das ist Ihre eigene Zeit (${zone}) — nicht die des Servers.`,
  sitesScheduleSave: "Veröffentlichung planen",
  sitesScheduleMove: "Auf diesen Zeitpunkt verschieben",
  sitesScheduleSaving: "Wird gespeichert…",
  sitesScheduleMissingMoment: "Wählen Sie zuerst Datum und Uhrzeit.",
  sitesScheduleSaveFailed: "Diese Website konnte nicht eingeplant werden.",
  sitesSchedulePending: (moment: string) =>
    `Diese Website veröffentlicht sich am ${moment} von selbst. Alles, was Sie bis dahin speichern, geht mit ihr online.`,
  sitesSchedulePublishingNow: "Diese Website wird gerade veröffentlicht.",
  sitesScheduleCancel: "Absagen",
  sitesScheduleCancelling: "Wird abgesagt…",
  sitesScheduleCancelFailed:
    "Die geplante Veröffentlichung konnte nicht abgesagt werden.",
  sitesScheduleCancelled: (moment: string) =>
    `Abgesagt. Diese Website veröffentlicht sich am ${moment} nicht, und an dem, was online ist, hat sich nichts geändert.`,
  sitesScheduleDone: (moment: string) =>
    `Diese Website hat sich am ${moment} von selbst veröffentlicht.`,
  sitesScheduleFailed: (moment: string, reason: string) =>
    `Diese Website konnte am ${moment} nicht veröffentlichen: ${reason}`,
  sitesPageAccess: "Seitenzugriff",
  sitesPagePasswordTitle: "Wer diese Seite öffnen kann",
  sitesPagePasswordLoading: "Es wird geprüft, wer diese Seite öffnen kann",
  sitesPagePasswordLoadFailed:
    "Ob diese Seite nach einem Passwort fragt, konnte nicht geprüft werden.",
  sitesPagePasswordUnknown:
    "Ob diese Seite Besucher nach einem Passwort fragt, ist gerade nicht bekannt.",
  sitesPagePasswordPublic: "Jeder im Internet kann diese Seite öffnen.",
  sitesPagePasswordPublicHint:
    "Geben Sie ihr ein Passwort, und nur die Menschen, denen Sie es geben, können sie lesen. Der Rest dieser Website bleibt öffentlich.",
  sitesPagePasswordProtected: (moment: string) =>
    `Nur wer das Passwort hat, kann diese Seite öffnen — festgelegt am ${moment}.`,
  sitesPagePasswordProtectedUndated:
    "Nur wer das Passwort hat, kann diese Seite öffnen.",
  sitesPagePasswordProtectedHint:
    "Alle anderen sehen einen Sperrbildschirm, der nichts von der Seite trägt, nicht einmal ihren Titel. Das Passwort öffnet sie für den Rest des Tages.",
  sitesPagePasswordEveryLanguage:
    "Das gilt für die Seite in jeder Sprache, in der sie veröffentlicht ist.",
  sitesPagePasswordProtect: "Diese Seite schützen",
  sitesPagePasswordChange: "Das Passwort ändern",
  sitesPagePasswordField: "Passwort",
  sitesPagePasswordFieldHint:
    "Niemand kann es Ihnen später vorlesen, auch wir nicht — ein vergessenes Passwort wird ersetzt, nicht wiederhergestellt.",
  sitesPagePasswordEffective:
    "Es wirkt sofort. Sie müssen die Website nicht neu veröffentlichen.",
  sitesPagePasswordShow: "Anzeigen",
  sitesPagePasswordHide: "Verbergen",
  sitesPagePasswordSaving: "Wird gespeichert…",
  sitesPagePasswordMissing: "Tippen Sie zuerst ein Passwort.",
  sitesPagePasswordSaveFailed: "Diese Seite konnte nicht geschützt werden.",
  sitesPagePasswordSaved:
    "Gespeichert. Besucher brauchen ab jetzt dieses Passwort, und wer die Seite mit dem alten geöffnet hat, wird neu gefragt.",
  sitesPagePasswordRemove: "Das Passwort entfernen",
  sitesPagePasswordRemoveConfirm: "Ja, öffentlich machen",
  sitesPagePasswordRemoveFailed: "Das Passwort konnte nicht entfernt werden.",
  sitesPagePasswordRemoved:
    "Das Passwort ist weg. Jeder im Internet kann diese Seite wieder öffnen.",
  sitesPagePasswordPreviewNote:
    "Besucher werden zuerst nach dem Passwort gefragt. Diese Vorschau zeigt die Seite, wie jemand mit Passwort sie sieht.",
  sitesPagePasswordBadge: "Passwort",

  // ---- Tranche 9: die Commerce-Hälfte von Sites — Katalog, Buchungen,
  // Tickets, Shop, Bestellungen, Sammlungen, eigener Code und Domains.
  // Damit ist der Katalog vollständig. Wortwahl: Besucher geben eine
  // „Bestellung“ auf — das alltägliche B2C-Wort; der förmliche „Auftrag“
  // bleibt dem Auftragsbuch in Lager. Eine Ticket-Veranstaltung ist die
  // „Veranstaltung“ (der Kalender-Termin bleibt der „Termin“), verkauft
  // werden „Plätze“, Ware liegt wie im Lager-Modul „auf Lager“, und der
  // KI-Vorschlag der Shop-Einrichtung wird wie jede Agent-Karte
  // „genehmigt“, während ein Domainpreis — ein Geldbetrag — „freigegeben“
  // wird.

  // Der Katalog: was die Website anbietet, mit Preisen.
  sitesCatalogs: "Katalog",
  sitesCatalogsHint:
    "Was diese Website anbietet — Gerichte, Zimmer, Leistungen, Kurse. Die Preise werden in dem Moment eingefroren, in dem Sie veröffentlichen.",
  sitesCatalogsLoading: "Der Katalog wird geladen…",
  sitesCatalogsLoadFailed:
    "Die Kataloge konnten nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  sitesCatalogLoadFailed:
    "Dieser Katalog ließ sich nicht öffnen. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  sitesNewCatalog: "Neuer Katalog",
  sitesCatalogNoneTitle: "Noch nichts im Angebot",
  sitesCatalogNoneBody:
    "Ein Katalog ist die Liste, die Ihre Website zeigt — und aus der sie, wenn Sie wollen, Bestellungen entgegennimmt. Beginnen Sie mit einem Namen und einer Währung; die Artikel kommen danach.",
  sitesCatalogOrdersOn: "Nimmt Bestellungen an",
  sitesCatalogOrdersOff: "Kein Bestellformular",
  sitesCatalogSettings: "Dieser Katalog",
  sitesCatalogSettingsHint:
    "Der Name ist nur für Sie; Besucher sehen die Artikel. Änderungen erreichen die veröffentlichte Website beim nächsten Veröffentlichen.",
  sitesCatalogName: "Name des Katalogs",
  sitesCatalogCurrency: "Währung",
  sitesCatalogCurrencyHint:
    "Drei Buchstaben, zum Beispiel EUR. Eine Änderung liest die schon geschriebenen Preise in der neuen Währung — umgerechnet wird nichts.",
  sitesCatalogOrders: "Bestellungen aus diesem Katalog annehmen",
  sitesCatalogOrdersHint:
    "Besucher bekommen unter der Liste ein Bestellformular. Auf der Website wird nichts bezahlt — die Bestellung landet in Ihrem Posteingang, und Sie bestätigen sie selbst. Sichtbar wird es beim nächsten Veröffentlichen.",
  sitesCatalogCreate: "Katalog anlegen",
  sitesCatalogSave: "Katalog speichern",
  sitesCatalogSaveFailed: "Der Katalog konnte nicht gespeichert werden.",
  sitesCatalogDelete: "Katalog löschen",
  sitesCatalogDeleteConfirm: "Löschen, mit allem darin",
  sitesCatalogDeleteHint:
    "Die Artikel und Gruppen gehen mit. Bereits veröffentlichte Seiten zeigen weiter, was sie zeigten, bis Sie erneut veröffentlichen.",
  sitesCatalogDeleteFailed: "Der Katalog konnte nicht gelöscht werden.",
  sitesCatalogGroups: "Gruppen",
  sitesCatalogGroupsHint:
    "Optional. Eine Gruppe ist eine Überschrift auf der Seite — Brote, Zimmer, Halbtagskurse.",
  sitesCatalogGroupName: "Name der Gruppe",
  sitesCatalogNewGroup: "Neue Gruppe",
  sitesCatalogNewGroupPlaceholder: "Brote",
  sitesCatalogAddGroup: "Gruppe hinzufügen",
  sitesCatalogGroupRemove: (name: string) => `Die Gruppe ${name} entfernen`,
  sitesCatalogGroupRemoveShort: "Entfernen",
  sitesCatalogGroupSaveFailed: "Die Gruppe konnte nicht gespeichert werden.",
  sitesCatalogGroupDeleteFailed: "Die Gruppe konnte nicht entfernt werden.",
  sitesCatalogItems: "Artikel",
  sitesCatalogItemsHint:
    "Alles, was dieser Katalog anbietet, in der Reihenfolge, in der die Seite es zeigt.",
  sitesCatalogAddItem: "Artikel hinzufügen",
  sitesCatalogNoItemsTitle: "Dieser Katalog ist leer",
  sitesCatalogNoItemsBody:
    "Fügen Sie hinzu, was Sie anbieten. Ein Name genügt für den Anfang — Preis, Foto und Beschreibung können folgen.",
  sitesCatalogNoPrice: "Preis auf Anfrage",
  sitesCatalogEdit: "Bearbeiten",
  sitesCatalogEditItem: (name: string) => `${name} bearbeiten`,
  sitesCatalogNewItem: "Neuer Artikel",
  sitesCatalogSaveItem: "Artikel speichern",
  sitesCatalogItemSubtitle:
    "Er erscheint auf der Website beim nächsten Veröffentlichen.",
  sitesCatalogItemName: "Name",
  sitesCatalogItemHandle: "Kurzname",
  sitesCatalogItemHandlePlaceholder: "Aus dem Namen",
  sitesCatalogItemHandleHint:
    "Der kurze Name für Links und auf Bestellungen. Lassen Sie ihn leer, und wir bilden einen aus dem Namen.",
  sitesCatalogItemPrice: (currency: string) => `Preis (${currency})`,
  sitesCatalogItemPriceHint:
    "Schreiben Sie ihn wie auf eine Karte — 4,50 oder 4.50. Leer lassen für Preis auf Anfrage.",
  sitesCatalogItemPriceNote: "Neben dem Preis",
  sitesCatalogItemPriceNoteHint:
    "Ein kurzer Zusatz — pro Nacht, ab, pro Person.",
  sitesCatalogItemGroup: "Gruppe",
  sitesCatalogItemNoGroup: "Keine Gruppe",
  sitesCatalogItemDescription: "Beschreibung",
  sitesCatalogItemPhoto: "Foto",
  sitesCatalogItemPhotoNone: "Noch kein Foto",
  sitesCatalogItemPhotoNoneHint:
    "Ein Artikel ohne Foto erscheint trotzdem — mit Name, Preis und Beschreibung.",
  sitesCatalogItemPhotoAdd: "Foto hinzufügen",
  sitesCatalogItemPhotoReplace: "Ersetzen",
  sitesCatalogItemPhotoRemove: "Das Foto entfernen",
  sitesCatalogItemPhotoPreview: "Das Foto dieses Artikels",
  sitesCatalogItemPhotoAlt: "Was das Foto zeigt",
  sitesCatalogItemPhotoAltHint:
    "Wird von Screenreadern vorgelesen. Beschreiben Sie das Bild — nicht den Namen, der darunter steht.",
  sitesCatalogItemPhotoAltMissing:
    "Noch hat niemand dieses Foto beschrieben; bis dahin greift die Karte auf den Artikelnamen zurück.",
  sitesCatalogItemAvailability: "Verfügbarkeit",
  sitesCatalogAvailabilityHint:
    "Ausverkauft erscheint weiterhin — markiert und nicht bestellbar. Ausgeblendet wird gar nicht veröffentlicht.",
  sitesCatalogAvailable: "Verfügbar",
  sitesCatalogSoldOut: "Ausverkauft",
  sitesCatalogHidden: "Ausgeblendet",
  sitesCatalogItemSaveFailed: "Der Artikel konnte nicht gespeichert werden.",
  sitesCatalogItemDelete: "Löschen",
  sitesCatalogItemDeleteConfirm: "Ja, löschen",
  sitesCatalogItemDeleteLabel: (name: string) => `${name} löschen`,
  sitesCatalogItemDeleteConfirmLabel: (name: string) => `Ja, löschen: ${name}`,
  sitesCatalogItemDeleteFailed: "Der Artikel konnte nicht gelöscht werden.",
  sitesSectionCatalog: "Katalog",
  sitesSectionCatalogDesc: "Was Sie anbieten, mit Preisen, aus Ihrem Katalog.",
  sitesCatalogSectionHeading: "Überschrift darüber",
  sitesCatalogSectionChoose: "Welcher Katalog",
  sitesCatalogSectionGroup: "Welche Gruppe",
  sitesCatalogSectionAllGroups: "Alles im Katalog",
  sitesCatalogSectionGroupHint:
    "Zeigen Sie eine Gruppe auf dieser Seite — die Mittagskarte, die Doppelzimmer — oder alles.",
  sitesCatalogSectionGoneGroup: (handle: string) =>
    `${handle} (keine Gruppe mehr)`,
  sitesCatalogSectionOneGroup: (handle: string) => `Eine Gruppe: ${handle}`,
  sitesCatalogSectionNoCatalogs: "Diese Website hat noch keinen Katalog",
  sitesCatalogSectionNoCatalogsHint:
    "Ein Katalog enthält, was Sie anbieten, mit den Preisen. Legen Sie einen an, und dieser Abschnitt kann ihn zeigen.",
  sitesCatalogSectionOrdersOn:
    "Dieser Katalog nimmt Bestellungen an, deshalb trägt die veröffentlichte Seite unter der Liste ein Bestellformular. Bestellungen landen im Bestelleingang dieser Website.",
  sitesCatalogSectionOrdersOff:
    "Dieser Katalog nimmt keine Bestellungen an, deshalb zeigt die Seite nur die Liste. Das Bestellen ist ein Schalter am Katalog, nicht an diesem Abschnitt.",

  // Buchungen: was Besucher buchen können, und der Kalender dahinter.
  sitesBookings: "Buchungen",
  sitesBookingsHint:
    "Was Besucher auf dieser Website buchen können — eine Beratung, eine Besichtigung, einen Tisch. Jede Buchung landet direkt in einem Ihrer Kalender.",
  sitesBookingsLoading: "Die buchbaren Leistungen werden geladen…",
  sitesBookingsLoadFailed:
    "Die buchbaren Leistungen konnten nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  sitesNewBooking: "Neue buchbare Leistung",
  sitesBookingNoneTitle: "Noch kann nichts gebucht werden",
  sitesBookingNoneBody:
    "Eine buchbare Leistung ist eine Sache, für die ein Besucher sich eine Zeit nehmen kann. Sagen Sie, wie lange sie dauert und wann Sie dafür geöffnet haben; die freien Zeiten werden aus Ihrem Kalender errechnet.",
  sitesBookingNoCalendarTitle: "Kein Kalender, in den gebucht werden kann",
  sitesBookingNoCalendarBody:
    "Eine Buchung ist ein Termin in einem Ihrer Kalender, also muss es einen Kalender geben, dem Sie Termine hinzufügen dürfen. Legen Sie im Kalender einen an, und er erscheint hier.",
  sitesBookingSettings: "Diese Leistung",
  sitesBookingSettingsHint:
    "Alles, was einem Besucher angeboten wird. Änderungen erreichen die veröffentlichte Website beim nächsten Veröffentlichen.",
  sitesBookingName: "Was gebucht wird",
  sitesBookingDescription: "Beschreibung",
  sitesBookingWhere: "Wo es stattfindet",
  sitesBookingWherePlaceholder: "Zweiter Stock, bitte klingeln",
  sitesBookingWhereLine: (place: string) => `Wo: ${place}`,
  sitesBookingCalendar: "Gebucht in",
  sitesBookingCalendarHint:
    "Termine werden in diesen Kalender geschrieben, und Zeiten, in denen Sie dort schon belegt sind, werden nie angeboten.",
  sitesBookingCalendarReadOnly: (name: string) =>
    `${name} — nur zum Lesen mit Ihnen geteilt`,
  sitesBookingCalendarGone: "Kalender nicht mehr verfügbar",
  sitesBookingCalendarGoneHint:
    "Der Kalender, in den diese Leistung gebucht wurde, ist nicht mehr erreichbar — er wurde gelöscht, oder die Freigabe wurde zurückgezogen. Bis Sie einen anderen wählen, bietet die veröffentlichte Seite gar keine Zeiten an.",
  sitesBookingOpenAgenda: "Den Kalender öffnen und die Termine verwalten",
  sitesBookingLength: "Dauer (Minuten)",
  sitesBookingBuffer: "Pause danach (Minuten)",
  sitesBookingNotice: "Kürzeste Vorlaufzeit (Minuten)",
  sitesBookingHorizon: "Buchbar im Voraus (Tage)",
  sitesBookingTimeZone: "Zeitzone",
  sitesBookingTimeZoneHint:
    "Die Uhr, nach der Ihre Öffnungszeiten geschrieben sind, als IANA-Name wie Europe/Brussels. Termine wandern mit der Uhr, wenn die Sommerzeit wechselt.",
  sitesBookingHours: "Wann Sie dafür geöffnet haben",
  sitesBookingHoursHint:
    "Ein leerer Kalender ist kein geöffneter Tag. Diese Fenster sind das Angebot; was schon im Kalender steht, wird davon abgezogen.",
  sitesBookingDay: "Tag",
  sitesBookingFrom: "Von",
  sitesBookingUntil: "Bis",
  sitesBookingAddWindow: "Fenster hinzufügen",
  sitesBookingRemoveWindow: (window: string) => `${window} entfernen`,
  sitesBookingNoHours:
    "Noch keine Öffnungszeiten — nichts kann gebucht werden.",
  sitesBookingQuestions: "Was Sie bei der Buchung fragen",
  sitesBookingQuestionsHint:
    "Name und E-Mail-Adresse werden immer erfragt und stehen nicht in dieser Liste. Ergänzen Sie nur, was genau diese Buchung braucht.",
  sitesBookingQuestionLabel: "Frage",
  sitesBookingQuestionLabelPlaceholder: "Telefonnummer",
  sitesBookingQuestionKey: "Gespeichert als",
  sitesBookingQuestionKind: "Art der Antwort",
  sitesBookingQuestionText: "Eine Zeile",
  sitesBookingQuestionLongText: "Mehrere Zeilen",
  sitesBookingQuestionPhone: "Telefonnummer",
  sitesBookingQuestionChoice: "Eine aus einer Liste",
  sitesBookingQuestionOptions: "Die angebotenen Antworten",
  sitesBookingQuestionOptionsPlaceholder: "Schnitt, Farbe, beides",
  sitesBookingQuestionRequired: "Muss beantwortet werden",
  sitesBookingAddQuestion: "Frage hinzufügen",
  sitesBookingRemoveQuestion: (question: string) =>
    `Die Frage ${question} entfernen`,
  sitesBookingActive: "Buchungen dafür annehmen",
  sitesBookingActiveHint:
    "Ausgeschaltet bleibt die Leistung genau, wie sie ist, und die veröffentlichte Seite sagt, dass sie vorerst keine Buchungen annimmt.",
  sitesBookingCreate: "Leistung anlegen",
  sitesBookingSave: "Leistung speichern",
  sitesBookingSaveFailed:
    "Die buchbare Leistung konnte nicht gespeichert werden.",
  sitesBookingDelete: "Leistung löschen",
  sitesBookingDeleteConfirm: "Ja, löschen",
  sitesBookingDeleteHint:
    "Termine, die schon in Ihrem Kalender stehen, bleiben genau, wie sie sind — nichts hier sagt einen ab. Bereits veröffentlichte Seiten bieten die Leistung weiter an, bis Sie erneut veröffentlichen.",
  sitesBookingDeleteFailed:
    "Die buchbare Leistung konnte nicht gelöscht werden.",
  sitesBookingMinutes: (minutes: number) => `${minutes} Minuten`,
  sitesBookingOff: "Nimmt keine Buchungen an",
  sitesBookingPreview: "Was ein Besucher sieht",
  sitesBookingPreviewHint:
    "Das Angebot, wie die veröffentlichte Seite es nennt. Die freien Zeiten selbst werden gegen Ihren Kalender errechnet, sobald jemand fragt.",
  sitesBookingUnnamed: "Leistung ohne Titel",
  sitesBookingAsksNothingExtra:
    "Besucher werden nach Name und E-Mail-Adresse gefragt.",
  sitesBookingAsksAlso: (questions: string) =>
    `Besucher werden nach Name und E-Mail-Adresse gefragt, und außerdem: ${questions}.`,
  sitesBookingPublishHint:
    "Auf der Website erscheint es, sobald eine Seite einen Buchungsabschnitt dafür trägt und Sie veröffentlichen.",
  sitesBookingOffPreview:
    "Diese Leistung ist ausgeschaltet, deshalb wird die Seite sagen, dass sie vorerst keine Buchungen annimmt.",
  sitesSectionBooking: "Buchung",
  sitesSectionBookingDesc:
    "Lassen Sie Besucher eine Zeit bei Ihnen buchen, direkt in Ihren Kalender.",
  sitesBookingSectionHeading: "Überschrift darüber",
  sitesBookingSectionChoose: "Was hier gebucht werden kann",
  sitesBookingSectionNoServices: "Diese Website hat noch nichts zu buchen",
  sitesBookingSectionNoServicesHint:
    "Eine buchbare Leistung sagt, wie lange sie dauert, wann Sie dafür geöffnet haben und in welchen Kalender sie geht. Legen Sie eine an, und dieser Abschnitt kann sie anbieten.",
  sitesBookingSectionOffOption: (name: string) =>
    `${name} (nimmt keine Buchungen an)`,
  sitesBookingSectionLength: (minutes: number) =>
    `Besucher wählen eine freie Zeit von ${minutes} Minuten. Die Zeiten kommen aus Ihrem Kalender, sobald jemand fragt — nicht aus dieser Seite.`,
  sitesBookingSectionOff:
    "Diese Leistung ist ausgeschaltet, deshalb wird die veröffentlichte Seite sagen, dass sie vorerst keine Buchungen annimmt.",
  sitesBookingSectionGone:
    "Die Leistung, die dieser Abschnitt anbot, ist weg. Wählen Sie eine andere, sonst wird das nächste Veröffentlichen abgelehnt.",

  // Der Ticketshop: Veranstaltungen mit Datum, Plätzen und einem Artikel
  // von der Preisliste.
  sitesSectionTickets: "Tickets",
  sitesSectionTicketsDesc:
    "Die Tür zu Ihrem Ticketshop. Was im Verkauf ist, Preise und Plätze bleiben aktuell.",
  sitesTicketSectionHeading: "Überschrift darüber",
  sitesTicketSectionBody: "Ihre eigenen Worte über dem Link",
  sitesTicketSectionNoEvents: "Noch ist nichts im Verkauf",
  sitesTicketSectionNoEventsHint:
    "Der veröffentlichte Abschnitt verweist auf Ihren Ticketshop. Legen Sie eine Veranstaltung an, damit es etwas zu kaufen gibt.",
  sitesTicketSectionHint:
    "Der veröffentlichte Abschnitt verweist auf Ihren Ticketshop; Veranstaltungen, Preise und Plätze werden live gelesen, wenn ein Besucher kommt.",
  sitesTicketSectionOnSale: (count: number) =>
    count === 1
      ? "1 Veranstaltung ist im Verkauf."
      : `${count} Veranstaltungen sind im Verkauf.`,
  sitesTickets: "Tickets",
  sitesTicketsLoadFailed:
    "Die Veranstaltungen konnten nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  sitesNoTicketEventsTitle: "Noch keine Veranstaltungen",
  sitesNoTicketEventsBody:
    "Eine Ticket-Veranstaltung verkauft Plätze für einen Artikel Ihrer Preisliste, an einem Datum. Shop, Kasse und das Zählen der Plätze sind schon gebaut — legen Sie die erste Veranstaltung an, und Ihre Website kann sie verkaufen.",
  sitesTicketNoProducts: "Noch steht nichts auf Ihrer Preisliste",
  sitesTicketNoProductsHint:
    "Eine Veranstaltung verkauft Plätze für einen Artikel Ihrer Preisliste, zu dessen eigenem Preis. Legen Sie den Artikel zuerst in Rechnungen an; Name und Preis bleiben dort und werden hier nie kopiert.",
  sitesNewTicketEvent: "Neue Veranstaltung",
  sitesNewTicketEventSubtitle:
    "Ein Datum, als was ein Platz verkauft wird, und wie viele Plätze es gibt.",
  sitesTicketCreateSubmit: "Veranstaltung anlegen",
  sitesTicketCreateFailed: "Die Veranstaltung konnte nicht angelegt werden.",
  sitesTicketEventProduct: "Als was ein Platz verkauft wird",
  sitesTicketEventProductHint:
    "Ein Artikel Ihrer Preisliste. Name und Preis werden live gelesen, nie kopiert.",
  sitesTicketProductOption: (name: string, price: string) =>
    `${name} — ${price}`,
  sitesTicketEventStartsAt: "Wann sie beginnt",
  sitesTicketEventCapacity: "Plätze",
  sitesTicketEventCapacityHint:
    "Mehr geht immer. Weniger endet bei den Plätzen, die schon verkauft oder vergeben sind.",
  sitesTicketCapacityTitle: "Die Plätze ändern",
  sitesTicketCapacitySubtitle: (taken: number) =>
    taken === 1
      ? "1 Platz ist schon verkauft oder vergeben."
      : `${taken} Plätze sind schon verkauft oder vergeben.`,
  sitesTicketCapacitySubmit: "Plätze speichern",
  sitesTicketCapacityFailed: "Die Platzzahl konnte nicht geändert werden.",
  sitesTicketChangeCapacity: "Plätze…",
  sitesTicketDelete: "Löschen",
  sitesTicketChangeCapacityFor: (event: string) => `Plätze für ${event} ändern`,
  sitesTicketDeleteFor: (event: string) => `${event} löschen`,
  sitesTicketDeleteConfirm: "Wirklich löschen?",
  sitesTicketDeleteHint:
    "Eine Veranstaltung, für die niemand gekauft hat, verschwindet. Sobald ein Platz verkauft ist, ist die Veranstaltung der Nachweis des Verkaufs und bleibt.",
  sitesTicketDeleteFailed: "Die Veranstaltung konnte nicht gelöscht werden.",
  sitesTicketWhen: "Wann",
  sitesTicketWhat: "Was",
  sitesTicketPrice: "Preis",
  sitesTicketSeats: "Plätze",
  sitesTicketSeatsCell: (sold: number, remaining: number, capacity: number) =>
    `${sold} verkauft · ${remaining} von ${capacity} frei`,
  sitesTicketHeld: (held: number) =>
    held === 1 ? "(1 gerade an der Kasse)" : `(${held} gerade an der Kasse)`,
  sitesTicketGoneProduct: "Steht nicht mehr auf der Preisliste",
  sitesAssistantSuggestedTickets: "Kann ich online Tickets kaufen?",

  // Das Schaufenster des Shops: Lagerware von der Preisliste, live gelesen.
  sitesSectionShop: "Shop",
  sitesSectionShopDesc:
    "Die Tür zu Ihrem Shop. Was im Verkauf ist, Preise und Bestand bleiben aktuell.",
  sitesShopSectionHeading: "Überschrift darüber",
  sitesShopSectionBody: "Ihre eigenen Worte über dem Link",
  sitesShopSectionNoItems: "Noch ist nichts im Shop",
  sitesShopSectionNoItemsHint:
    "Der Block verweist auf Ihre Shop-Seite. Führen Sie auf dem Shop-Bildschirm ein Produkt mit Bestand auf, und es erscheint dort.",
  sitesShopSectionHint:
    "Der Block verweist auf Ihre Shop-Seite. Was im Verkauf ist, Preise und Bestand werden live gelesen — in der Seite selbst ist nichts gespeichert.",
  sitesShopSectionListed: (count: number) =>
    count === 1 ? "1 Produkt ist im Shop." : `${count} Produkte sind im Shop.`,
  sitesAssistantSuggestedShop: "Was verkaufen Sie?",
  sitesShop: "Shop",
  sitesShopLoadFailed:
    "Der Shop konnte nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  sitesShopAddProduct: "Produkt hinzufügen",
  sitesShopAddSubtitle:
    "Wählen Sie ein Produkt mit Bestand aus Ihrer Preisliste. Name, Preis und Bestand gehören weiter Rechnungen und Lager — der Shop führt es nur auf.",
  sitesShopAddSubmit: "In den Shop aufnehmen",
  sitesShopAddFailed: "Das Produkt konnte nicht hinzugefügt werden.",
  sitesShopProduct: "Was verkauft wird",
  sitesShopProductHint:
    "Nur Lagerware von Ihrer Preisliste kann hier verkauft werden.",
  sitesShopProductOption: (name: string, price: string, units: number) =>
    units === 1
      ? `${name} — ${price} (1 auf Lager)`
      : `${name} — ${price} (${units} auf Lager)`,
  sitesShopColWhat: "Was",
  sitesShopColPrice: "Preis",
  sitesShopColShelf: "Auf Lager",
  sitesShopGoneProduct: "Steht nicht mehr auf der Preisliste",
  sitesShopNotStocked: "Keine Lagerware mehr",
  sitesShopUnits: (units: number) =>
    units === 1 ? "1 Stück" : `${units} Stück`,
  sitesShopRemove: "Entfernen",
  sitesShopRemoveFor: (product: string) => `${product} aus dem Shop entfernen`,
  sitesShopRemoveConfirm: "Wirklich entfernen?",
  sitesShopRemoveHint:
    "Entfernen nimmt das Produkt nur aus dem Schaufenster. Bereits aufgegebene Bestellungen behalten es.",
  sitesShopRemoveFailed: "Das Produkt konnte nicht entfernt werden.",
  sitesShopNoProducts: "Noch liegt nichts zum Verkaufen auf Lager",
  sitesShopNoProductsHint:
    "Der Shop verkauft Lagerware von Ihrer Preisliste. Legen Sie in Rechnungen einen Artikel an (oder lassen Sie sich von der Shop-Einrichtung eine Liste vorschlagen), buchen Sie Bestand ein, und er erscheint hier.",
  sitesShopEmptyTitle: "Ihr Schaufenster ist leer",
  sitesShopEmptyBody:
    "Führen Sie ein Produkt mit Bestand auf, und Besucher können es auf Ihrer Website kaufen — bezahlt auf der Seite des Zahlungsanbieters.",
  sitesShopAllListed: "Jedes Produkt mit Bestand ist schon im Shop.",
  sitesShopDeliveryRate: (price: string) =>
    `Die Lieferung kostet ${price} pro Bestellung.`,
  sitesShopDeliveryFree: "Die Lieferung ist kostenlos.",
  sitesCommerceReadOnly:
    "Nur wer diese Website besitzt, kann ändern, was sie verkauft und berechnet — Sie können schauen, nicht ändern.",
  sitesShopDeliveryChange: "Lieferung ändern…",
  sitesShopDeliveryTitle: "Lieferung pro Bestellung",
  sitesShopDeliverySubtitle:
    "Ein Pauschalpreis pro Bestellung, neben der Ware berechnet. Die MwSt. folgt der Ware.",
  sitesShopDeliveryLabel: (currency: string) => `Lieferpreis (${currency})`,
  sitesShopDeliveryHint: "0 heißt: Die Lieferung ist kostenlos.",
  sitesShopDeliverySave: "Lieferung speichern",
  sitesShopDeliveryFailed: "Der Lieferpreis konnte nicht gespeichert werden.",

  // Die Shop-Einrichtung: ein Vorschlag zum Genehmigen, wie jede
  // Agent-Karte — nichts existiert, bevor Sie es genehmigen.
  sitesShopSetup: "Shop-Einrichtung",
  sitesShopSetupSubtitle:
    "Beschreiben Sie Ihr Geschäft, und Sie bekommen eine vorgeschlagene Preisliste, MwSt.-Behandlung und einen Lieferpreis zum Prüfen. Nichts wird angelegt, bevor Sie es genehmigen.",
  sitesShopSetupLoadFailed:
    "Die Shop-Einrichtung konnte nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  sitesShopSetupDescribeLabel: "Was verkaufen Sie?",
  sitesShopSetupDescribeHint:
    "Nennen Sie, was Sie verkaufen, und die Preise, die Sie verlangen. Genannte Preise werden übernommen, wie sie dastehen — alles andere bleibt leer oder eine markierte Vermutung, die Sie bestätigen.",
  sitesShopSetupPropose: "Einrichtung vorschlagen",
  sitesShopSetupProposeFailed:
    "Es ließ sich keine Einrichtung vorschlagen. Versuchen Sie es erneut.",
  sitesShopSetupUnconfigured:
    "Dieser Arbeitsbereich hat keinen KI-Anbieter eingerichtet, deshalb kann hier nichts vorgeschlagen werden — legen Sie Ihre Preisliste stattdessen von Hand an.",
  sitesShopSetupManualPath: "Lieber von Hand?",
  sitesShopSetupManualTickets: "Ticket-Veranstaltungen verwalten",
  sitesShopSetupManualCatalogs: "Kataloge verwalten",
  sitesShopSetupExisting: (count: number) =>
    count === 1
      ? "Ihre Preisliste hat schon 1 Artikel. Genehmigen fügt hinzu — ersetzt wird nie etwas."
      : `Ihre Preisliste hat schon ${count} Artikel. Genehmigen fügt hinzu — ersetzt wird nie etwas.`,
  sitesShopSetupProposalTitle: "Der Vorschlag",
  sitesShopSetupProposalIntro:
    "Prüfen Sie jede Zeile, bevor Sie genehmigen. Angezeigte Preise standen in Ihrer Beschreibung; Leerstellen füllen Sie selbst, und jeder MwSt.-Satz ist eine Vermutung zum Bestätigen.",
  sitesShopSetupInclude: (name: string) => `„${name}“ anlegen`,
  sitesShopSetupItemName: "Name",
  sitesShopSetupItemUnit: "Einheit",
  sitesShopSetupItemPrice: (currency: string) => `Preis (${currency})`,
  sitesShopSetupVatLabel: "MwSt. %",
  sitesShopSetupVatGuessBadge: "MwSt. ist eine Vermutung",
  sitesShopSetupNameMissing:
    "Jeder angehakte Artikel braucht einen Namen, bevor Sie genehmigen.",
  sitesShopSetupPriceMissing:
    "Ihre Beschreibung nannte keinen Preis — tragen Sie einen ein, bevor Sie genehmigen.",
  sitesShopSetupVatMissing:
    "Tragen Sie für jeden angehakten Artikel einen MwSt.-Prozentsatz ein, bevor Sie genehmigen.",
  sitesShopSetupKindStock: "Ware",
  sitesShopSetupKindDated: "Tickets",
  sitesShopSetupKindService: "Dienstleistung",
  sitesShopSetupShippingTitle: "Lieferung",
  sitesShopSetupShippingNotNeeded:
    "Nichts in diesem Vorschlag wird verschickt, also gibt es keinen Lieferpreis zu setzen.",
  sitesShopSetupShippingLabel: (currency: string) =>
    `Pauschaler Lieferpreis pro Bestellung (${currency})`,
  sitesShopSetupShippingMissing:
    "Ware wird verschickt, aber Ihre Beschreibung nannte keinen Lieferpreis — tragen Sie einen ein, bevor Sie genehmigen.",
  sitesShopSetupShippingCurrent: (price: string) => `Derzeit ${price}.`,
  sitesShopSetupShippingSaved: "Lieferpreis gespeichert.",
  sitesShopSetupShippingFailed:
    "Der Lieferpreis konnte nicht gespeichert werden.",
  sitesShopSetupNothingIncluded:
    "Nichts ist angehakt — haken Sie mindestens einen Artikel an, um ihn anzulegen.",
  sitesShopSetupApprove: (count: number) =>
    count === 1
      ? "Genehmigen — 1 Artikel anlegen"
      : `Genehmigen — ${count} Artikel anlegen`,
  sitesShopSetupRetry: "Erneut versuchen",
  sitesShopSetupDiscard: "Vorschlag verwerfen",
  sitesShopSetupCreated: "Angelegt",
  sitesShopSetupCreateFailed: "Dieser Artikel konnte nicht angelegt werden.",
  sitesShopSetupDone: (count: number) =>
    count === 1
      ? "1 Artikel steht jetzt auf Ihrer Preisliste."
      : `${count} Artikel stehen jetzt auf Ihrer Preisliste.`,
  sitesShopSetupNextTickets:
    "Die Veranstaltungen planen, für die Tickets verkauft werden",

  // Der Bestelleingang: worum Besucher gebeten haben, und was als
  // Nächstes damit geschieht.
  sitesOrders: "Bestellungen",
  sitesOrdersLoadFailed:
    "Die Bestellungen konnten nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  sitesOrdersExport: "Als CSV exportieren",
  sitesOrdersExporting: "Wird exportiert…",
  sitesOrdersExportFailed: "Die Bestellungen konnten nicht exportiert werden.",
  sitesNoOrdersTitle: "Noch keine Bestellungen",
  sitesNoOrdersBody:
    "Zeigt eine veröffentlichte Seite einen Katalog, der Bestellungen annimmt, landet hier, worum Besucher bitten — mit dem, was sie wollen, ihren Angaben und der Summe.",
  sitesOrderList: "Bestellungen",
  sitesOrderDetail: "Diese Bestellung",
  sitesOrderFilter: "Zeigen",
  sitesOrderFilterAll: "Alle",
  sitesOrderFilterOption: (label: string, count: number) =>
    `${label} (${count})`,
  sitesOrderFilterEmpty: "Keine Bestellungen in diesem Stand.",
  sitesOrderStatus: "Wo diese Bestellung steht",
  sitesOrderStatusNew: "Neu",
  sitesOrderStatusConfirmed: "Bestätigt",
  sitesOrderStatusFulfilled: "Erledigt",
  sitesOrderStatusCancelled: "Storniert",
  sitesOrderStatusFailed: "Die Bestellung konnte nicht umgestellt werden.",
  sitesOrderCatalog: "Aus",
  sitesOrderPhone: "Telefon",
  sitesOrderItem: "Artikel",
  sitesOrderQuantity: "Wie viele",
  sitesOrderUnitPrice: "Einzeln",
  sitesOrderLineTotal: "Zeile",
  sitesOrderTotal: "Summe",
  sitesOrderLinesCaption: "Was bestellt wurde",
  sitesOrderLineNoPrice: "Auf Anfrage",
  sitesOrderQuotedHint:
    "Ein Artikel ohne Preis zählt nicht zur Summe — nennen Sie den Preis selbst, wenn Sie antworten.",
  sitesOrderLineCount: (count: number) =>
    count === 1 ? "1 Artikel" : `${count} Artikel`,
  sitesOrderDelete: "Bestellung löschen",
  sitesOrderDeleteConfirm: "Endgültig löschen",
  sitesOrderDeleteHint:
    "Diese Bestellung enthält den Namen, die Telefonnummer und die Wünsche einer Person. Löschen entfernt alles davon — es gibt kein Zurück.",
  sitesOrderDeleteFailed: "Die Bestellung konnte nicht gelöscht werden.",

  // Sammlungen: Zeilen aus alo Base als wiederverwendbare Karten.
  sitesCollections: "Sammlungen",
  sitesCollectionsHint:
    "Machen Sie aus einer Tabelle in alo Base wiederverwendbare Karten für Ihre Website.",
  sitesConnectTable: "Tabelle verbinden",
  sitesCollectionsLoading: "Sammlungen werden geladen…",
  sitesCollectionsLoadFailed:
    "Die Sammlungen konnten nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  sitesCollectionEmptyTitle: "Verbinden Sie Ihre erste Tabelle",
  sitesCollectionEmptyBody:
    "Wählen Sie eine alo Base, ordnen Sie ihre Spalten einmal zu, und verwenden Sie diese Zeilen auf jeder Seite wieder.",
  sitesCollectionNoBasesTitle: "Legen Sie zuerst eine alo Base an",
  sitesCollectionNoBasesBody:
    "Sammlungen lesen Zeilen aus alo Base. Legen Sie in Drive eine Base an und kommen Sie dann hierher zurück, um sie zu verbinden.",
  sitesCollectionOpenDrive: "Drive öffnen",
  sitesCollectionName: "Name der Sammlung",
  sitesCollectionBase: "alo Base",
  sitesCollectionTable: "Tabelle",
  sitesCollectionChooseBase: "Base wählen",
  sitesCollectionChooseTable: "Tabelle wählen",
  sitesCollectionRows: (count: number) =>
    count === 1 ? "1 Zeile" : `${count} Zeilen`,
  sitesCollectionConnectedTo: (base: string, table: string) =>
    `${base} / ${table}`,
  sitesCollectionSourceUnavailable:
    "Wählen Sie die Base und die Tabelle, deren Zeilen auf der Website erscheinen sollen.",
  sitesCollectionEdit: (name: string) => `${name} bearbeiten`,
  sitesCollectionMapping: "Spalten dem Website-Inhalt zuordnen",
  sitesCollectionMappingHint:
    "Der Titel ist Pflicht. Alles andere ist optional und kann später dazukommen.",
  sitesCollectionOptional: "Optional",
  sitesCollectionNotMapped: "Nicht zeigen",
  sitesCollectionNoCompatibleField: "Diese Tabelle braucht eine Textspalte",
  sitesCollectionTitleField: "Titel",
  sitesCollectionSlugField: "Seitenpfad",
  sitesCollectionSummaryField: "Zusammenfassung",
  sitesCollectionBodyField: "Text",
  sitesCollectionImageField: "Bild",
  sitesCollectionLinkField: "Link",
  sitesCollectionDateField: "Veröffentlichungsdatum",
  sitesCollectionSave: "Sammlung speichern",
  sitesCollectionSaving: "Wird gespeichert…",
  sitesCollectionSaveFailed:
    "Die Sammlung wurde nicht gespeichert. Nichts hat sich geändert; prüfen Sie die markierte Zuordnung und versuchen Sie es erneut.",
  sitesCollectionDisconnect: "Trennen",
  sitesCollectionDisconnectConfirm: "Jetzt trennen",
  sitesCollectionDisconnectHint:
    "Die Base und alle ihre Zeilen bleiben in Drive.",
  sitesCollectionDisconnectFailed:
    "Die Sammlung ist noch verbunden. Entfernen Sie sie von allen Seiten, die sie verwenden, und versuchen Sie es dann erneut.",
  sitesCollectionPreview: "Aktuelle Zeilen",
  sitesCollectionPreviewHint:
    "Genau das wird die nächste Veröffentlichung aus Base lesen.",
  sitesCollectionPreviewLoading: "Die aktuellen Base-Zeilen werden geladen",
  sitesCollectionPreviewFailed:
    "Diese Zeilen ließen sich nicht anzeigen. Korrigieren Sie den Base-Wert, den der Server nennt, und versuchen Sie es erneut.",
  sitesCollectionPreviewSaveTitle: "Speichern Sie, um diese Zeilen zu sehen",
  sitesCollectionPreviewSaveBody:
    "Einmal verbunden, prüfen dieselben Veröffentlichungsregeln wie auf der Live-Website jede Zeile hier.",
  sitesCollectionPreviewEmptyTitle:
    "Diese Tabelle hat noch keine vollständigen Zeilen",
  sitesCollectionPreviewEmptyBody:
    "Geben Sie einer Zeile in Base einen Titel, und sie erscheint hier von selbst.",
  sitesCollectionPreviewLinked: "Öffnet einen Link",
  sitesSectionCollection: "Sammlung",
  sitesSectionCollectionDesc:
    "Ein wiederverwendbares Raster aus Zeilen von alo Base.",
  sitesCollectionSectionHeading: "Überschrift des Abschnitts",
  sitesCollectionSectionChoose: "Welche Sammlung gezeigt wird",
  sitesCollectionSectionNoConnections:
    "Verbinden Sie eine Tabelle, bevor Sie diesen Abschnitt hinzufügen",
  sitesCollectionSectionNoConnectionsHint:
    "Die Sammlung bleibt wiederverwendbar — dieselbe Base kann mehr als eine Seite versorgen.",

  // Der versiegelte Eigener-Code-Block: die Worte leisten, was die CSP
  // erzwingt.
  sitesSectionCustomCode: "Eigener Code",
  sitesSectionCustomCodeDesc:
    "Ihr eigenes HTML, CSS und JavaScript, versiegelt in einem Rahmen ohne Ausweg.",
  sitesCustomCodeBoundaryTitle: "Was dieser Block kann und was nicht",
  sitesCustomCodeBoundarySealed:
    "Er läuft abgeschottet von Ihrer Website: Er kann weder die Seite um sich herum lesen noch Ihre Besucher, noch irgendetwas, das sie anderswo eingetippt haben.",
  sitesCustomCodeBoundaryNoNetwork:
    "Er hat kein Netz. Nichts lädt von einer anderen Adresse — kein Embed, keine Schrift, kein Statistik-Skript — und genau das hält diese Website frei von einem Cookie-Banner.",
  sitesCustomCodeBoundaryYours:
    "Es ist Ihr Code, veröffentlicht genau, wie Sie ihn geschrieben haben. Wir prüfen nicht, was er tut, und der Assistent schreibt und ändert ihn nicht.",
  sitesCustomCodeHeadingHint:
    "Wird von der Seite über dem Block gezeigt, in der Schrift Ihrer Website. Leer lassen für einen Block, der für sich steht.",
  sitesCustomCodeFrameTitle: "Was dieser Block ist",
  sitesCustomCodeFrameTitleHint:
    "Wird Besuchern mit Screenreader vorgelesen — „Ein Countdown zur laufenden Röstung“, nicht „Rahmen“.",
  sitesCustomCodeHtml: "Markup",
  sitesCustomCodeHtmlHint:
    "Der Rumpf des Blocks. Das Dokument darum herum — seine Policy, seine Style- und Script-Blöcke — wird für Sie geschrieben.",
  sitesCustomCodeCss: "Stil",
  sitesCustomCodeCssHint: "Gilt nur in diesem Block. Optional.",
  sitesCustomCodeJs: "Skript",
  sitesCustomCodeJsHint:
    "Läuft nur in diesem Block, auf dem Gerät des Besuchers.",
  sitesCustomCodeCapabilities: "Was der Block darf",
  sitesCustomCodeCapabilitiesHint:
    "Alles ist aus, bis Sie es einschalten, und nur diese zwei lassen sich einschalten.",
  sitesCustomCodeScripts: "Ein Skript ausführen",
  sitesCustomCodeScriptsHint:
    "Ohne dies ist der Block Markup und Stil: Nichts darin wird ausgeführt, was auch immer er sagt.",
  sitesCustomCodeScriptMissing:
    "Es gibt noch kein Skript zum Ausführen. Schreiben Sie eins, oder schalten Sie dies aus — eine Erlaubnis, hinter der nichts steht, wird abgelehnt.",
  sitesCustomCodeScriptDropped:
    "Ausgeschaltet — das Skript unten wird deshalb nicht mit dem Block gespeichert. Schalten Sie es wieder ein, um es zu behalten.",
  sitesCustomCodeImages: "Bilder zeigen, die im Markup mitgeschrieben sind",
  sitesCustomCodeImagesHint:
    "Für ein Bild, das ins Markup selbst geschrieben ist. Ein Bild von einer Adresse kann weiterhin nicht laden — nehmen Sie dafür einen Bild-Abschnitt.",
  sitesCustomCodeHeight: "Höhe auf der Seite (Pixel)",
  sitesCustomCodeHeightHint:
    "Ein versiegelter Block lässt sich von außen nicht messen, also sagen Sie, wie hoch er ist. Zwischen 40 und 2000.",
  sitesCustomCodeBytes: (used: number, max: number) =>
    `${used} von ${max} Bytes`,
  sitesCustomCodeBytesOver: (used: number, max: number) =>
    `${used} von ${max} Bytes — zu lang zum Speichern`,
  sitesCustomCodeTotalBytes: (used: number, max: number) =>
    `${used} von ${max} Bytes in diesem Block insgesamt`,

  // Domains: die Adressen, unter denen eine Website antwortet. Jeder Preis
  // wird zweimal gesagt — heute und jedes Jahr danach — und nichts heißt
  // fertig, bevor es fertig ist.
  sitesDomains: "Domains",
  sitesDomainsLoading: "Die Domains werden geladen…",
  sitesDomainsLoadFailed:
    "Die Domains dieser Website konnten nicht geladen werden. Prüfen Sie Ihre Verbindung und versuchen Sie es erneut.",
  sitesDomainAloAddress: "Diese Website ist immer erreichbar unter",
  sitesDomainOwned: "Eine Domain, die Sie schon besitzen",
  sitesDomainOwnedHint:
    "Fügen Sie die Domain hinzu, veröffentlichen Sie den gezeigten Eintrag bei Ihrem DNS-Anbieter und drücken Sie dann auf Prüfen. Für Ihre Besucher ändert sich nichts, bis sie verifiziert ist.",
  sitesDomainAddress: "Domain",
  sitesDomainPlaceholder: "beispiel.de",
  sitesDomainAdd: "Domain hinzufügen",
  sitesDomainAddFailed: "Diese Domain konnte nicht hinzugefügt werden.",
  sitesDomainNoneBody:
    "Noch ist keine eigene Domain verbunden. Fügen Sie eine hinzu, die Sie schon besitzen, oder kaufen Sie unten eine — und diese Website antwortet auch dort.",
  sitesDomainStatusPending: "Wartet auf den Eintrag",
  sitesDomainStatusVerified: "Verifiziert",
  sitesDomainStatusLive: "Liefert aus",
  sitesDomainCheck: "Prüfen",
  sitesDomainVerifyFailed: "Die Domain konnte nicht geprüft werden.",
  sitesDomainNotYet:
    "Der Eintrag ist noch nicht sichtbar. DNS-Änderungen brauchen ein paar Minuten, um sich zu verbreiten — lassen Sie den Eintrag stehen und prüfen Sie gleich noch einmal.",
  sitesDomainVerifiedNow: (domain: string) =>
    `${domain} ist verifiziert. Diese Website antwortet jetzt auch dort.`,
  sitesDomainRecordTitle:
    "Veröffentlichen Sie diesen Eintrag bei Ihrem DNS-Anbieter",
  sitesDomainRecordName: "Name",
  sitesDomainRecordType: "Typ",
  sitesDomainRecordValue: "Wert",
  sitesDomainRecordHint:
    "Lassen Sie den Eintrag stehen, bis die Prüfung gelingt. Manche DNS-Anbieter hängen die Domain selbst an den Namen an — tut Ihrer das, lassen Sie sie weg.",
  sitesDomainPointHint: (host: string) =>
    `Der letzte Schritt bei Ihrem DNS-Anbieter: Richten Sie die Domain mit einem CNAME auf ${host}. Eine Apex-Domain braucht stattdessen den ALIAS- oder ANAME-Eintrag Ihres Anbieters.`,
  sitesDomainCopy: "Kopieren",
  sitesDomainCopied: "Kopiert",
  sitesDomainRemove: "Entfernen",
  sitesDomainRemoveConfirm: "Ja, entfernen",
  sitesDomainRemoveHint:
    "alo antwortet unter dieser Domain nicht mehr. Die Domain selbst bleibt Ihre — bei der Registry wird nichts aufgegeben.",
  sitesDomainRemoveFailed: "Diese Domain konnte nicht entfernt werden.",

  sitesDomainBuy: "Eine Domain kaufen",
  sitesDomainBuyHint:
    "Suchen Sie einen Namen. Sie sehen, was er dieses Jahr kostet und was jedes Jahr danach, bevor irgendetwas gekauft wird.",
  sitesDomainSearchLabel: "Der Name, den Sie möchten",
  sitesDomainSearchPlaceholder: "acme",
  sitesDomainSearching: "Wird gesucht…",
  sitesDomainSearchInvite:
    "Tippen Sie einen Namen, um zu sehen, welche Endungen frei sind.",
  sitesDomainSearchFailed: "Dieser Name konnte nicht geprüft werden.",
  sitesDomainCatalogFailed: "Die Domainpreise konnten nicht geladen werden.",
  sitesDomainUnconfiguredTitle: "Domains kaufen ist hier nicht eingeschaltet",
  sitesDomainUnconfiguredBody:
    "Dieser Arbeitsbereich kann keine Domainnamen registrieren. Eine Domain, die Sie schon besitzen, können Sie trotzdem verbinden.",
  sitesDomainNotBuyable:
    "Dieser Arbeitsbereich kann Preise zeigen, aber noch keine Domain registrieren, weil keine Nameserver eingerichtet sind.",
  sitesDomainTestRegistrar: (name: string) =>
    `${name} ist ein Test-Registrar: Nichts wird berechnet, und kein echter Name wird registriert.`,
  sitesDomainRegistrarLine: (name: string, country: string) =>
    `Domains werden über ${name} (${country}) registriert. Preise ohne MwSt.`,
  sitesDomainAvailable: "Frei",
  sitesDomainTaken: "Schon registriert",
  sitesDomainBlocked: "Nicht zu verkaufen",
  sitesDomainUnsupportedEnding: "alo verkauft diese Endung nicht",
  sitesDomainPremium: "Premium-Name",
  sitesDomainPremiumHint:
    "Die Registry bepreist diesen Namen über dem üblichen Preis seiner Endung. Sein Verlängerungspreis ist der gezeigte, nicht der gewöhnliche.",
  sitesDomainPriceLine: (today: string, renewal: string) =>
    `${today} heute, dann ${renewal} pro Jahr`,
  sitesDomainChoose: "Diese Domain kaufen",

  sitesDomainPurchaseTitle: (domain: string) => `${domain} kaufen`,
  sitesDomainPurchaseSubtitle:
    "Auf wen die Domain registriert wird, und für wie lange. Den Preis geben Sie im nächsten Schritt frei; vorher wird nichts berechnet.",
  sitesDomainYears: "Bezahlt für",
  sitesDomainYearsHint:
    "Wie viele Jahre die erste Zahlung deckt. Danach geht es jahrweise weiter.",
  sitesDomainYearsOption: (years: number) =>
    years === 1 ? "1 Jahr" : `${years} Jahre`,
  sitesDomainAutoRenew: "Diese Domain automatisch verlängern",
  sitesDomainAutoRenewHint:
    "Eine Domain, die nicht verlängert wird, ist verloren, und jeder darf sie dann nehmen. Schalten Sie dies nur aus, wenn Sie selbst verlängern wollen.",
  sitesDomainAutoRenewOn: "Sie verlängert sich jedes Jahr automatisch.",
  sitesDomainAutoRenewOff:
    "Sie verlängert sich nicht automatisch: Sie müssen sie selbst verlängern, bevor sie abläuft, sonst verlieren Sie sie.",
  sitesDomainRegistrant: "Registriert auf",
  sitesDomainRegistrantHint:
    "Die Registry verlangt eine echte Person oder Firma, die erreichbar ist. Das geht an die Registry — auf Ihrer Website erscheint es nie.",
  sitesDomainRegistrantName: "Vollständiger Name",
  sitesDomainRegistrantOrganisation: "Firma (leer lassen, wenn es keine gibt)",
  sitesDomainRegistrantEmail: "E-Mail",
  sitesDomainRegistrantEmailHint:
    "Hierhin schreibt die Registry zu Ablauf und Verifizierung. Eine Adresse, die niemand liest, kostet die Domain.",
  sitesDomainRegistrantStreet: "Straße und Hausnummer",
  sitesDomainRegistrantPostalCode: "Postleitzahl",
  sitesDomainRegistrantCity: "Ort",
  sitesDomainRegistrantCountry: "Land",
  sitesDomainRegistrantCountryHint:
    "Der Zwei-Buchstaben-Ländercode, etwa de oder be.",
  sitesDomainRegistrantPhone: "Telefon",
  sitesDomainRegistrantPhoneHint: "In internationaler Form, etwa +49301234567.",
  sitesDomainRequirementEea:
    "Diese Endung wird nur an Inhaber im Europäischen Wirtschaftsraum verkauft.",
  sitesDomainRequirementCountry: (country: string) =>
    `Diese Endung wird nur an Inhaber in ${country} verkauft.`,
  sitesDomainSeePrice: "Den Preis sehen",
  sitesDomainQuoteFailed: "Diese Domain konnte nicht bepreist werden.",
  sitesDomainApproveTitle: "Diesen Preis freigeben",
  sitesDomainApproveSubtitle: (domain: string) =>
    `Was ${domain} kostet, vollständig, bevor irgendetwas berechnet wird.`,
  sitesDomainQuoteName: "Domain",
  sitesDomainQuoteTerm: "Bezahlt für",
  sitesDomainQuoteToday: "Heute",
  sitesDomainQuoteRenewal: "Jedes Jahr danach",
  sitesDomainApproveAction: (price: string) => `${price} freigeben`,
  sitesDomainApproveHint:
    "Mit der Freigabe ist festgehalten, dass Sie genau diesen Beträgen zugestimmt haben. Ändert sich der Preis, bevor er bezahlt ist, fragt alo erneut, statt einen anderen zu berechnen.",
  sitesDomainApproveFailed: "Dieser Preis konnte nicht freigegeben werden.",

  sitesDomainPurchases: "Hier gekaufte Domains",
  sitesDomainPurchasesHint:
    "Jede Domain, die diese Website zu kaufen begonnen hat, und wie weit sie gekommen ist.",
  sitesDomainPurchasesNone:
    "Für diese Website wurde noch keine Domain gekauft.",
  sitesDomainPurchasesLoadFailed:
    "Die Domainkäufe konnten nicht geladen werden.",
  sitesDomainRefresh: "Aktualisieren",
  sitesDomainTermPrice: (price: string, years: number) =>
    years === 1
      ? `${price} für das erste Jahr`
      : `${price} für die ersten ${years} Jahre`,
  sitesDomainRenewalLine: (price: string) => `danach ${price} pro Jahr`,
  sitesDomainApprovedOn: (when: string) => `Preis freigegeben am ${when}.`,
  sitesDomainAttempts: (attempts: number) =>
    `Registrierungsversuch ${attempts}; alo versucht es weiter.`,
  sitesDomainCancel: "Abbrechen",
  sitesDomainCancelConfirm: "Ja, abbrechen",
  sitesDomainCancelFailed: "Dieser Kauf konnte nicht abgebrochen werden.",
  sitesDomainStateQuoted: "Wartet auf Ihre Freigabe",
  sitesDomainStateApproved: "Freigegeben",
  sitesDomainStateAwaitingPayment: "Wartet auf die Zahlung",
  sitesDomainStatePaid: "Bezahlt",
  sitesDomainStateRegistering: "Wird registriert",
  sitesDomainStateRegistered: "Registriert",
  sitesDomainStateConfigured: "Online",
  sitesDomainStateFailed: "Nicht abgeschlossen",
  sitesDomainStateCancelled: "Abgebrochen",
  sitesDomainStepQuoted:
    "Berechnet wurde nichts. Geben Sie den Preis frei, und der Kauf geht weiter zur Zahlung.",
  sitesDomainStepApproved:
    "Sie haben diesen Preis freigegeben. Als Nächstes kommt die Zahlung: Sobald sie eingegangen ist, registriert alo die Domain und verbindet sie von selbst mit dieser Website.",
  sitesDomainStepAwaitingPayment:
    "Wartet darauf, dass die Zahlung eingeht. Die Registrierung startet von selbst, sobald das geschieht.",
  sitesDomainStepPaid:
    "Bezahlt. Die Registrierung startet innerhalb einer Minute.",
  sitesDomainStepRegistering:
    "Der Registrar registriert den Namen gerade jetzt.",
  sitesDomainStepRegistered: (domain: string) =>
    `${domain} ist auf Sie registriert. Sie wird mit dieser Website verbunden.`,
  sitesDomainStepConfigured: (domain: string) =>
    `${domain} ist registriert und liefert diese Website aus.`,
  sitesDomainStepFailed:
    "Dieser Kauf konnte nicht abgeschlossen werden. Dafür wird nichts weiter berechnet.",
  sitesDomainStepCancelled: "Abgebrochen. Berechnet wurde nichts.",
  sitesDomainOwnerOnly:
    "Nur wer diese Website besitzt, kann ihre Domainnamen kaufen und verwalten. Die Website selbst können Sie weiter bearbeiten und veröffentlichen.",
  billingImportPrices: "Preise importieren",
  billingPriceList: "Preisliste",
  billingColVat: "USt.",
  billingImportImageUnreadable: "Das Bild konnte nicht gelesen werden.",
  billingImportMissingName: "Name fehlt",
  billingImportInvalidPrice: "Ungültiger Preis",
  billingImportInvalidVat: "Ungültiger Umsatzsteuersatz",
  billingImportReadFailed:
    "Diese Preisliste konnte nicht gelesen werden. Versuchen Sie es mit einer CSV-, Excel-, PNG-, JPEG- oder WebP-Datei.",
  billingImportSaveFailed:
    "Der Import wurde beendet, bevor alle Artikel gespeichert werden konnten.",
  billingImportNotInFile: "Nicht in dieser Datei",
  billingImportTitle: "Preisliste importieren",
  billingImportItems: (count: number) => `${count} Artikel importieren`,
  billingImportViewPriceList: "Preisliste anzeigen",
  billingImportDropTitle: "Preisliste hier ablegen",
  billingImportDropHelp:
    "Excel- und CSV-Dateien werden direkt in Ihrem Browser gelesen. Bei einem Foto oder Screenshot extrahiert alo AI die Zeilen, damit Sie sie prüfen können.",
  billingImportSpreadsheetFormats: "CSV · XLSX",
  billingImportImageFormats: "PNG · JPEG · WebP",
  billingImportChooseFile: "Datei auswählen",
  billingImportReading: (name: string) => `${name} wird gelesen…`,
  billingImportRowsFound: (count: number) =>
    `${count} Zeilen gefunden. Prüfen Sie die Zuordnung und schließen Sie alles aus, was Sie nicht importieren möchten.`,
  billingImportReplaceFile: "Datei ersetzen",
  billingImportMatchColumns: "Spalten zuordnen",
  billingImportSku: "Artikelnummer",
  billingImportColumnLabel: (field: string) => `Spalte für ${field}`,
  billingImportChooseColumn: "Spalte auswählen",
  billingImportColumn: "Importieren",
  billingImportIncludeRow: (name: string) => `${name} importieren`,
  billingImportRow: (number: number) => `Zeile ${number}`,
  billingImportAlreadyExists: "Bereits vorhanden",
  billingImportReady: "Bereit",
  billingImportComplete: (count: number) =>
    `${count} Preislistenartikel importiert`,
  billingImportCompleteHelp:
    "Sie können jetzt in Angeboten, Rechnungen und geteilten Preisverbindungen verwendet werden.",
  colorPickerEyedropper: "Farbe vom Bildschirm aufnehmen",
  colorPickerHue: "Farbton",
  colorPickerChannelValue: (channel: string) => `${channel}-Wert`,
  colorPickerHex: "HEX",
  colorPickerHexColour: "Hexadezimalfarbe",
  colorPickerCopyHex: "Hexadezimalfarbe kopieren",
  colorPickerUseColour: (colour: string) => `${colour} verwenden`,
  colorPickerSaveColour: "Aktuelle Farbe speichern",
  colorPickerUseDefault: "Standardfarbe verwenden",
  billingEditProductImage: "Produktbild bearbeiten",
  billingCloseImageEditor: "Bildeditor schließen",
  billingApplyImage: "Bild übernehmen",
  billingPdfPreview: "PDF-Vorschau",
  billingQuotationPreview: "Angebotsvorschau",
  billingImagePdfHelp:
    "Diese Bildgröße und dieser Ausschnitt werden im PDF verwendet.",
  billingPdfPaperSizeA4: "A4",
  billingProductPdfPreview: "Produktbild in der PDF-Vorschau",
  billingCropStyle: "Bildausschnitt",
  billingFillFrame: "Rahmen ausfüllen",
  billingShowFullImage: "Ganzes Bild anzeigen",
  billingZoom: "Zoom",
  billingCustomZoom: "Benutzerdefinierter Zoom in Prozent",
  billingZoomHelp:
    "Verwenden Sie 50–90 %, um mehr vom Bild zu zeigen, oder mehr als 100 % für einen engeren Ausschnitt.",
  billingFocusArea: "Fokusbereich",
  billingCentre: "Mitte",
  billingTop: "Oben",
  billingBottom: "Unten",
  billingLeft: "Links",
  billingRight: "Rechts",
  billingProductImage: "Produktbild",
  billingProductImageHelp:
    "Wird im Kundenangebot neben diesem Artikel angezeigt.",
  billingReplaceImage: "Bild ersetzen",
  billingUploadImage: "Bild hochladen",
  billingRemoveImage: "Bild entfernen",
  billingProductDescription: "Produktbeschreibung",
  billingProductDescriptionPlaceholder:
    "Fügen Sie Spezifikationen, Materialien, Leistungsumfang oder andere hilfreiche Details hinzu…",
  billingConnectionsSyncNow: "Jetzt synchronisieren",
  billingConnectionsConnectSupplier: "Lieferantenpreise verbinden",
  billingConnectionsConnectPrices: "Preise verbinden",
  billingConnectionsEasyOption: "Beginnen Sie mit der einfachsten Option",
  billingConnectionsEasyOptionHelp:
    "Wenn Ihr Lieferant alo verwendet, fügen Sie dessen Einladungslink ein. Authentifizierung und Produktfelder übernehmen wir automatisch.",
  billingConnectionsSupplier: "Lieferant",
  billingConnectionsSupplierPlaceholder: "Firmenname des Lieferanten",
  billingConnectionsType: "Verbindungstyp",
  billingConnectionsChooseConnection: "Verbindung auswählen",
  billingConnectionsInvitationLink: "Einladungslink",
  billingConnectionsInvitationHelp:
    "Ihr Lieferant erstellt diesen Link unter Von mir geteilt in seinem alo-Arbeitsbereich.",
  billingConnectionsInvitationPlaceholder: "alo-Einladungslink einfügen",
  billingConnectionsAccessKey: "Zugriffsschlüssel",
  billingConnectionsAccessKeyHelp:
    "Bleibt vertraulich und wird nie in Ihren Kundendokumenten angezeigt.",
  billingConnectionsAccessKeyPlaceholder:
    "Schlüssel Ihres Lieferanten einfügen",
  billingConnectionsReady: "Verbindung ist bereit",
  billingConnectionsTestPreview: "Testen und Vorschau anzeigen",
  billingConnectionsSyncApprovals: "Synchronisierung und Freigaben",
  billingConnectionsSyncApprovalsHelp:
    "Wählen Sie, wann Preise geprüft werden und welche Änderungen freigegeben werden müssen.",
  billingConnectionsCheckUpdates: "Auf Aktualisierungen prüfen",
  billingConnectionsChooseSchedule: "Zeitplan auswählen",
  billingConnectionsApplyChanges: "Preisänderungen übernehmen",
  billingConnectionsChooseApproval: "Freigaberegel auswählen",
  billingConnectionsChangeLimit: "Grenzwert für automatische Änderungen",
  billingConnectionsChangeLimitHelp:
    "Änderungen über diesem Prozentsatz warten auf eine Freigabe.",
  billingConnectionsProductMatching: "Produktzuordnung",
  billingConnectionsProductMatchingHelp:
    "Legen Sie fest, wie Lieferantenprodukte vorhandenen Artikeln in Ihrem Katalog zugeordnet werden.",
  billingConnectionsMatchBy: "Produkte zuordnen nach",
  billingConnectionsChooseMatching: "Zuordnungsmethode auswählen",
  billingConnectionsNewProducts: "Neue Lieferantenprodukte",
  billingConnectionsChooseAction: "Aktion auswählen",
  billingConnectionsFieldMapping: "Zuordnung der Lieferantenfelder",
  billingConnectionsFieldMappingHelp:
    "Geben Sie die Feldnamen dieses Lieferanten ein. alo schlägt sie nach der ersten Vorschau vor.",
  billingConnectionsSkuField: "SKU-Feld",
  billingConnectionsNameField: "Namensfeld",
  billingConnectionsNetPriceField: "Nettopreisfeld",
  billingConnectionsCurrencyField: "Währungsfeld",
  billingConnectionsCustomHeader:
    "Benutzerdefinierter Authentifizierungs-Header",
  billingConnectionsHeaderName: "Header-Name",
  billingConnectionsHeaderValue: "Header-Wert",
  billingConnectionsHeaderValuePlaceholder: "Sicheren Wert eingeben",
  billingConnectionsSharePrices: "Meine Preise teilen",
  billingConnectionsCreateSecure: "Sichere Verbindung erstellen",
  billingConnectionsYouControl: "Sie bestimmen genau, was dieser Kunde erhält",
  billingConnectionsYouControlHelp:
    "Interne Einkaufskosten, Lieferantennamen und Margen werden niemals einbezogen.",
  billingConnectionsClientPartner: "Kunde oder Partner",
  billingConnectionsCompanyName: "Firmenname",
  billingConnectionsDeliveryMethod:
    "Wie soll die Verbindung hergestellt werden?",
  billingConnectionsChooseDelivery: "Übertragungsart auswählen",
  billingConnectionsPricesToShare: "Zu teilende Preise",
  billingConnectionsChoosePrices: "Preise auswählen",
  billingConnectionsChooseProducts: "Produkte aus der Preisliste auswählen",
  billingConnectionsSearchPriceList: "Ihre Preisliste durchsuchen",
  billingConnectionsNoProducts:
    "Keine Produkte in der Preisliste entsprechen dieser Suche.",
  billingConnectionsLoadingPriceList: "Ihre Preisliste wird geladen…",
  billingConnectionsSecureCreated: "Sichere Preisverbindung erstellt",
  billingConnectionsSendTo: (company: string) =>
    `Senden Sie diese Angaben an ${company}. Der Zugriff kann jederzeit pausiert oder widerrufen werden.`,
  billingConnectionsKeyShownOnce:
    "Der vollständige Schlüssel wird nur bei der Erstellung angezeigt.",
  billingConnectionsCopy: "Kopieren",
  billingConnectionsConnected: "Verbunden",
  billingConnectionsExpired: "Abgelaufen",
  billingConnectionsActionNeeded: "Handlungsbedarf",
  billingConnectionsPaused: "Pausiert",
  billingConnectionsIndustrialComponentsEur: "Industriekomponenten · EUR",
  billingConnectionsChangesReady: (count: number) =>
    `${count} Preisänderungen stehen zur Prüfung bereit`,
  billingConnectionsUpdatedMinutesAgo: (count: number) =>
    `Vor ${count} Minuten aktualisiert`,
  billingConnectionsDaily: "Täglich",
  billingConnectionsMetalsSheetEur: "Metalle und Bleche · EUR",
  billingConnectionsSupplierRenew:
    "Der Lieferant muss diese Verbindung erneuern",
  billingConnectionsUpdatedDaysAgo: (count: number) =>
    `Zuletzt vor ${count} Tagen aktualisiert`,
  billingConnectionsWholesaleContract: "Großhandelskatalog · Vertragspreise",
  billingConnectionsWorkspaceReceivesApproved:
    "Der alo-Arbeitsbereich erhält freigegebene Preisänderungen",
  billingConnectionsUsedHoursAgo: (count: number) =>
    `Vor ${count} Stunde verwendet`,
  billingConnectionsOnApproval: "Nach Freigabe",
  billingConnectionsProjectSupplyEur: "Projektlieferpreise · EUR",
  billingConnectionsApiExpiryDemo:
    "Der externe API-Zugriff läuft am 30. September 2026 ab",
  billingConnectionsUsedYesterday: "Gestern verwendet",
  billingConnectionsLive: "Live",
  billingConnectionsSupplierCatalogueEur: "Lieferantenkatalog · EUR",
  billingConnectionsNoChangesAttention:
    "Keine Preisänderungen erfordern Ihre Aufmerksamkeit",
  billingConnectionsConnectedNow: "Gerade verbunden",
  billingConnectionsHourly: "Stündlich",
  billingConnectionsWeekly: "Wöchentlich",
  billingConnectionsManual: "Manuell",
  billingConnectionsLivePriceListAutomatic:
    "Live-Preisliste · Automatisch aktualisiert",
  billingConnectionsSelectedPriceItems: "Ausgewählte Preislistenpositionen",
  billingConnectionsWaitingClient:
    "Warten auf die Annahme durch den Kunden in alo",
  billingConnectionsExternalReady:
    "Der externe API-Zugriff kann geteilt werden",
  billingConnectionsCreatedNow: "Gerade erstellt",
  billingConnectionsReceivedByMe: "Von mir empfangen",
  billingConnectionsSharedByMe: "Von mir geteilt",
  billingConnectionsUpdatedNow: "Gerade aktualisiert",
  billingConnectionsUpToDate: (company: string) =>
    `${company} ist auf dem neuesten Stand.`,
  billingConnectionsNowSupplying: (company: string) =>
    `${company} liefert jetzt Preise an diesen Arbeitsbereich.`,
  billingConnectionsNowReceiving: (company: string) =>
    `${company} erhält jetzt Preise aus diesem Arbeitsbereich.`,
  billingConnectionsDisconnectTitle: "Preisverbindung trennen?",
  billingConnectionsDisconnectReceived: (company: string) =>
    `${company} sendet keine Lieferantenpreise mehr an diesen Arbeitsbereich. Bestehende Preise bleiben erhalten, werden aber nicht mehr automatisch aktualisiert.`,
  billingConnectionsDisconnectShared: (company: string) =>
    `${company} erhält keine Preise mehr aus diesem Arbeitsbereich. Bestehende Preise bleiben erhalten, werden aber nicht mehr automatisch aktualisiert.`,
  billingConnectionsDisconnect: "Trennen",
  billingConnectionsKeepConnected: "Verbunden lassen",
  billingConnectionsTitle: "Preisverbindungen",
  billingPriceConnections: "Preisverbindungen",
  billingVat: "MwSt.",
  billingConnectionsSubtitle:
    "Empfangen Sie aktuelle Lieferantenkosten und teilen Sie ausgewählte Verkaufspreise sicher mit Ihren Kunden.",
  billingConnectionsDirection: "Richtung der Preisverbindung",
  billingConnectionsSearch: "Verbindungen durchsuchen",
  billingConnectionsDismiss: "Schließen",
  billingConnectionsNoMatches: "Keine passenden Verbindungen",
  billingConnectionsNoMatchesHelp:
    "Versuchen Sie eine andere Suche oder erstellen Sie eine neue Preisverbindung.",
  quoteStudioScanToSave: "Scannen und speichern",
  quoteStudioBuildTitle: "Erstellen Sie Ihr Angebot",
  quoteStudioBuildHelp:
    "Fügen Sie Inhalte direkt hinzu. Änderungen werden automatisch gespeichert.",
  quoteStudioCompanyLogo: "Firmenlogo",
  quoteStudioAddress: "Adresse",
  quoteStudioContact: "Kontakt",
  quoteStudioVatId: "USt-IdNr.",
  quoteStudioCompanyNumber: "Handelsregisternummer",
  quoteStudioQuotation: "Angebot",
  quoteStudioPreparedFor: "Erstellt für",
  quoteStudioIssued: "Ausgestellt",
  quoteStudioValidUntil: "Gültig bis",
  quoteStudioEditHeader: "Kopfzeile bearbeiten",
  quoteStudioTableName: "Tabellenname",
  quoteStudioPricingTable: "Preistabelle",
  quoteStudioTableSettings: "Tabelleneinstellungen",
  quoteStudioEditBlock: "Block bearbeiten",
  quoteStudioMoveUp: "Nach oben verschieben",
  quoteStudioMoveDown: "Nach unten verschieben",
  quoteStudioDuplicate: "Duplizieren",
  quoteStudioDelete: "Löschen",
  quoteStudioHeadingLevel: "Überschriftenebene",
  quoteStudioHeading1: "Überschrift 1",
  quoteStudioHeading2: "Überschrift 2",
  quoteStudioHeading3: "Überschrift 3",
  quoteStudioSectionHeading: "Abschnittsüberschrift",
  quoteStudioParagraph: "Absatz",
  quoteStudioWriteParagraph: "Schreiben Sie einen Absatz…",
  quoteStudioImportantStatement:
    "Fügen Sie ein Kundenzitat oder einen wichtigen Hinweis hinzu…",
  quoteStudioAttribution: "Quellenangabe (optional)",
  quoteStudioQuoteAttribution: "Quellenangabe zum Zitat",
  quoteStudioSectionText: "Abschnittstext",
  quoteStudioSectionTextPlaceholder:
    "Schreiben Sie die Informationen, die Ihr Kunde benötigt…",
  quoteStudioListLayout: "Listenlayout",
  quoteStudioListLayoutHelp:
    "Verteilen Sie längere Listen auf übersichtliche Spalten.",
  quoteStudioColumns: "Spalten",
  quoteStudioChooseColumns: "Spalten auswählen",
  quoteStudioWriteItem: "Eintrag verfassen",
  quoteStudioMoveItemUp: "Eintrag nach oben verschieben",
  quoteStudioMoveItemDown: "Eintrag nach unten verschieben",
  quoteStudioRemoveItem: "Eintrag entfernen",
  quoteStudioAddItemBelow: "Eintrag darunter hinzufügen",
  quoteStudioListFormatting: "Formatierung des Listeneintrags",
  quoteStudioBold: "Fett",
  quoteStudioItalic: "Kursiv",
  quoteStudioEditContentBlock: "Inhaltsblock bearbeiten",
  quoteStudioChangesImmediate: "Änderungen werden sofort im Angebot angezeigt.",
  quoteStudioDone: "Fertig",
  quoteStudioComposeImageText: "Bild und Text zusammenstellen",
  quoteStudioComposeImageTextHelp:
    "Ordnen Sie den Block an und sehen Sie genau, wie er im Angebot erscheint.",
  quoteStudioLayoutTools: "Layoutoptionen",
  quoteStudioLayoutToolsHelp:
    "Wählen Sie, wie dieser Inhaltsblock im Angebot dargestellt wird.",
  quoteStudioComposition: "Anordnung",
  quoteStudioImageFrame: "Bildrahmen",
  quoteStudioFit: "Einpassung",
  quoteStudioImage: "Bild",
  quoteStudioImageDescriptionPlaceholder:
    "Beschreiben Sie das gezeigte Produkt, Projekt oder Ergebnis.",
  quoteStudioCaption: "Bildunterschrift",
  quoteStudioCaptionPlaceholder: "Optionale kurze Bildunterschrift",
  quoteStudioTextTools: "Textwerkzeuge",
  quoteStudioTextFormatting: "Textformatierung",
  quoteStudioBulletList: "Aufzählung",
  quoteStudioNumberedList: "Nummerierte Liste",
  quoteStudioColumnWidth: "Spaltenbreite",
  quoteStudioSideBySideOnly: "Nur nebeneinander",
  quoteStudioZoom: "Zoom",
  quoteStudioReset: "Zurücksetzen",
  quoteStudioZoomOut: "Verkleinern",
  quoteStudioZoomIn: "Vergrößern",
  quoteStudioInformationTable: "Informationstabelle",
  quoteStudioInformationTableHelp:
    "Benennen Sie die Spalten um und fügen Sie so viele Zeilen oder Spalten wie nötig hinzu.",
  quoteStudioTableColumnCount: "Anzahl der Tabellenspalten",
  quoteStudioRowActions: "Zeilenaktionen",
  quoteStudioEnterValue: "Wert eingeben",
  quoteStudioAddFirstRow:
    "Fügen Sie die erste Zeile hinzu, um diese Tabelle zu beginnen.",
  quoteStudioAddRowBelow: "Zeile darunter hinzufügen",
  quoteStudioAddContentA11y: "Angebotsinhalt hinzufügen",
  quoteStudioAddContentBelow: "Inhalt darunter hinzufügen",
  quoteStudioAddContent: "Inhalt hinzufügen",
  quoteStudioAddToQuotation: "Zum Angebot hinzufügen",
  quoteStudioAddToQuotationHelp:
    "Wählen Sie, was als Nächstes im Dokument erscheinen soll.",
  quoteStudioCloseBlockPicker: "Blockauswahl schließen",
  quoteStudioSearchBlocks: "Blöcke suchen…",
  quoteStudioSearchBlocksA11y: "Angebotsblöcke suchen",
  quoteStudioNoMatchingBlocks: "Keine passenden Blöcke",
  quoteStudioTryAnotherName: "Versuchen Sie einen anderen Suchbegriff.",
  quoteStudioFirstBlockHelp:
    "Fügen Sie als ersten Block Text, eine Überschrift oder ein Bild hinzu.",
  quoteStudioClose: "Schließen",
  quoteStudioBrandMark: "Markenauftritt",
  quoteStudioBrandMarkHelp: "Wird oben im Kundenangebot angezeigt.",
  quoteStudioQuoteLogo: "Angebotslogo",
  quoteStudioUploadLogo: "Ihr Logo hochladen",
  quoteStudioRemove: "Entfernen",
  quoteStudioQrTitle: "Kontakt-QR-Code",
  quoteStudioQrHelp: "Kunden können Ihre Kontaktdaten scannen und speichern.",
  quoteStudioShowQr: "Kontakt-QR-Code anzeigen",
  quoteStudioPlacement: "Position",
  quoteStudioPlacementHelp:
    "Wählen Sie die Position des Codes neben Ihren Firmendaten.",
  quoteStudioSize: "Größe",
  quoteStudioSizeHelp:
    "Prüfen Sie den Platzbedarf des QR-Codes in der Kopfzeile.",
  quoteStudioQrColour: "Farbe des QR-Codes",
  quoteStudioCompanyInformation: "Unternehmensangaben",
  quoteStudioCompanyLinkedHelp:
    "Diese Werte stammen aus Abrechnung → Ihre Angaben.",
  quoteStudioOverrideHelp:
    "Eine Änderung erstellt eine abweichende Angabe für dieses Angebot.",
  quoteStudioUseYourDetails: "Ihre Angaben verwenden",
  quoteStudioLinkedYourDetails: "Mit Ihren Angaben verknüpft",
  quoteStudioCompanyName: "Firmenname",
  quoteStudioCompanyNamePlaceholder: "Name Ihres Unternehmens",
  quoteStudioWebsite: "Website",
  quoteStudioWebsitePlaceholder: "www.unternehmen.de",
  quoteStudioEmail: "E-Mail",
  quoteStudioEmailPlaceholder: "vertrieb@unternehmen.de",
  quoteStudioPhone: "Telefon",
  quoteStudioVatPlaceholder: "Umsatzsteuer-Identifikationsnummer",
  quoteStudioCompanyNumberPlaceholder: "Handelsregisternummer",
  quoteStudioCustomerInformation: "Kundenangaben",
  quoteStudioCustomerInformationHelp:
    "Wird in der Angebotskopfzeile unter Erstellt für angezeigt.",
  quoteStudioCustomerOverrideHelp:
    "Eine Änderung erstellt nur für dieses Angebot eine abweichende Angabe.",
  quoteStudioUseSelectedCustomer: "Ausgewählten Kunden verwenden",
  quoteStudioLinkedSelectedCustomer: "Mit ausgewähltem Kunden verknüpft",
  quoteStudioCustomerCompanyPlaceholder: "Name des Kundenunternehmens",
  quoteStudioContactPerson: "Kontaktperson",
  quoteStudioContactNamePlaceholder: "Name der Kontaktperson",
  quoteStudioCustomerEmailPlaceholder: "kontakt@kunde.de",
  quoteStudioCustomerVatPlaceholder: "USt-IdNr. des Kunden",
  quoteStudioOnFinalization: "Bei Finalisierung",
  quoteStudioDaysAfterIssue: (days: string) => `${days} Tage nach Ausstellung`,
  quoteStudioSupportingText: "Begleittext",
  quoteStudioHeading: "Überschrift",
  quoteStudioHeadingHelp: "Wählen Sie H1, H2 oder H3",
  quoteStudioQuote: "Zitat",
  quoteStudioParagraphHelp: "Fügen Sie einen erläuternden Text hinzu",
  quoteStudioQuoteHelp: "Heben Sie eine Aussage hervor",
  quoteStudioBulletListHelp: "Listen Sie die wichtigsten Punkte auf",
  quoteStudioNumberedListHelp: "Zeigen Sie geordnete Schritte",
  quoteStudioImageHelp: "Laden Sie ein Bild hoch und ordnen Sie es an",
  quoteStudioPricingTableHelp: "Gruppieren Sie Produkte und Leistungen",
  quoteStudioTable: "Tabelle",
  quoteStudioTableHelp: "Erstellen Sie flexible Zeilen und Spalten",
  quoteStudioDivider: "Trennlinie",
  quoteStudioDividerHelp: "Trennen Sie Dokumentabschnitte",
  quoteStudioDividerSettings: "Trennlinie einstellen",
  quoteStudioDividerAppearance: "Darstellung der Trennlinie",
  quoteStudioDividerAppearanceHelp:
    "Legen Sie fest, wie diese Trennlinie im Kundenangebot erscheint.",
  quoteStudioDividerStyle: "Linienstil",
  quoteStudioDividerStyleHelp:
    "Wählen Sie, wie die Trennlinie in Ihrem Angebot dargestellt wird.",
  quoteStudioDividerSolid: "Durchgezogen",
  quoteStudioDividerDashed: "Gestrichelt",
  quoteStudioDividerDotted: "Gepunktet",
  quoteStudioDividerThickness: "Linienstärke",
  quoteStudioDividerThicknessHelp: "Wählen Sie die Linienstärke.",
  quoteStudioDividerFine: "Fein",
  quoteStudioDividerMedium: "Mittel",
  quoteStudioDividerBold: "Kräftig",
  quoteStudioDividerWidth: "Linienbreite",
  quoteStudioDividerWidthHelp: "Legen Sie die Breite der Trennlinie fest.",
  quoteStudioDividerColour: "Linienfarbe",
  quoteStudioChooseDividerColour: "Farbe der Trennlinie auswählen",
  quoteStudioChooseColour: "Farbe auswählen",
  quoteStudioHexColour: "Hex-Farbe",
  quoteStudioCopyColour: "Farbe kopieren",
  quoteStudioCategoryText: "Text",
  quoteStudioEditQuotationHeader: "Angebotskopf bearbeiten",
  quoteStudioCustomizeQuotation: "Angebot anpassen",
  quoteStudioChangesSavedAutomatically:
    "Änderungen werden automatisch gespeichert.",
  quoteStudioReplace: "Ersetzen",
  quoteStudioChooseFile: "Datei auswählen",
  quoteStudioLeft: "Links",
  quoteStudioRight: "Rechts",
  quoteStudioSmall: "Klein",
  quoteStudioMedium: "Mittel",
  quoteStudioLarge: "Groß",
  quoteStudioQrPlacementA11y: (side: string) =>
    `QR-Code ${side.toLowerCase()} platzieren`,
  quoteStudioQrColourHelp:
    "Wählen Sie für zuverlässiges Scannen eine dunkle Farbe",
  quoteStudioPhonePlaceholder: "+49 30 123 456",
  quoteStudioAddressPlaceholder:
    "Straße und Hausnummer\nPostleitzahl und Ort\nLand",
  quoteStudioHeaderStyle: "Kopfstil",
  quoteStudioHeaderStyleHelp:
    "Wählen Sie eine professionelle Gestaltung. Ihre gespeicherten Unternehmensdaten werden automatisch eingefügt.",
  quoteStudioHeaderArrangement: "Anordnung des Kopfbereichs",
  quoteStudioHeaderArrangementHelp:
    "Wählen Sie, auf welcher Seite Ihre Unternehmensidentität steht.",
  quoteStudioLogoLeft: "Logo links",
  quoteStudioLogoRight: "Logo rechts",
  quoteStudioLogoLeftHelp:
    "Unternehmensidentität links, Angebotsdetails gegenüber.",
  quoteStudioLogoRightHelp:
    "Unternehmensidentität rechts, Angebotsdetails gegenüber.",
  quoteStudioColumnBalance: "Spaltenaufteilung",
  quoteStudioColumnBalanceHelp:
    "Legen Sie fest, wie viel Platz Unternehmen und Kunde erhalten.",
  quoteStudioColumnBalanceA11y: "Spaltenaufteilung des Angebotskopfs",
  quoteStudioColumnRatioA11y: (company: string, customer: string) =>
    `Unternehmen ${company} Prozent, Kunde ${customer} Prozent`,
  quoteStudioDocumentPalette: "Dokumentpalette",
  quoteStudioDocumentPaletteHelp:
    "Steuern Sie die Farben der Kundenseite und der Preistabellen.",
  quoteStudioResetDefaults: "Standardwerte wiederherstellen",
  quoteStudioDocument: "Dokument",
  quoteStudioDocumentHelp: "Marke, Seite, Kopfbereich und Text.",
  quoteStudioAccent: "Akzent",
  quoteStudioAccentHelp: "Markenaktionen und Hervorhebungen",
  quoteStudioContactIcons: "Kontaktsymbole",
  quoteStudioContactIconsHelp: "Symbole für E-Mail, Telefon und Website",
  quoteStudioPage: "Seite",
  quoteStudioPageHelp: "Hintergrund für Kunden",
  quoteStudioHeader: "Kopfbereich",
  quoteStudioHeaderHelp: "Hintergrund des Kopfbereichs",
  quoteStudioText: "Text",
  quoteStudioTextHelp: "Primärtext",
  quoteStudioBulletDots: "Aufzählungspunkte",
  quoteStudioListMarkers: "Listenmarkierungen",
  quoteStudioNumberMarkers: "Nummernmarkierungen",
  quoteStudioNumberedSteps: "Nummerierte Schritte",
  quoteStudioPricingTables: "Preistabellen",
  quoteStudioPricingTablesHelp:
    "Sorgen Sie für gut erfassbare Überschriften und Zeilen.",
  quoteStudioTableHeading: "Tabellenkopf",
  quoteStudioTableHeadingHelp: "Hintergrund des Tabellenkopfs",
  quoteStudioTableRows: "Tabellenzeilen",
  quoteStudioTableRowsHelp: "Standardhintergrund der Zeilen",
  quoteStudioTypography: "Typografie",
  quoteStudioTypographyHelp:
    "Wählen Sie den Lesestil, der am besten zu Ihrer Marke passt.",
  quoteStudioProposal: "Angebot",
  quoteStudioCloseTableSettings: "Tabelleneinstellungen schließen",
  quoteStudioTableChangesSavedAutomatically:
    "Tabellenänderungen werden automatisch gespeichert.",
  quoteStudioChooseLayout: "Layout auswählen",
  quoteStudioChooseLayoutHelp:
    "Wählen Sie einen Ausgangspunkt und passen Sie anschließend die sichtbaren Inhalte und Spalten an.",
  quoteStudioCompact: "Kompakt",
  quoteStudioCompactHelp: "Nur Namen und Preise",
  quoteStudioDetailed: "Detailliert",
  quoteStudioDetailedHelp: "Beschreibungen mit optionalen Bildern",
  quoteStudioCatalogue: "Katalog",
  quoteStudioCatalogueHelp: "Größere Produktbilder und Details",
  quoteStudioProductContent: "Produktinhalt",
  quoteStudioProductContentHelp:
    "Optionale Informationen zu jedem Produkt oder jeder Dienstleistung.",
  quoteStudioProductImages: "Produktbilder",
  quoteStudioProductImagesHelp: "Fügen Sie jeder Tabellenzeile ein Bild hinzu",
  quoteStudioProductDescriptions: "Produktbeschreibungen",
  quoteStudioProductDescriptionsHelp:
    "Fügen Sie unter jeder Position Spezifikationen oder den Leistungsumfang hinzu",
  quoteStudioVisibleColumns: "Sichtbare Spalten",
  quoteStudioVisibleColumnsHelp:
    "Produktname und Angebotssumme bleiben immer sichtbar.",
  quoteStudioUnit: "Einheit",
  quoteStudioQuantity: "Menge",
  quoteStudioUnitPrice: "Einzelpreis",
  quoteStudioVatRate: "MwSt.-Satz",
  quoteStudioLineTotal: "Positionssumme",
  quoteStudioShowColumn: (label: string) =>
    `Spalte ${label.toLowerCase()} anzeigen`,
  quoteStudioPricingTableTotals: "Summen der Preistabelle",
  quoteStudioPricingTableTotalsHelp:
    "Wählen Sie, wie die Betragsübersicht unter jeder Preistabelle erscheint. Jede Tabelle behält ihre eigene Zwischensumme.",
  quoteStudioSummaryCard: "Übersichtskarte",
  quoteStudioSummaryCardHelp: "Kompakt und rechtsbündig",
  quoteStudioFullWidth: "Volle Breite",
  quoteStudioFullWidthHelp: "Nutzt die gesamte Tabellenbreite ausgewogen",
  quoteStudioTableFooter: "Tabellenfuß",
  quoteStudioTableFooterHelp: "Schließt optisch an die Zeilen an",
  quoteStudioTotalsStyle: "Summenstil",
  quoteStudioTotalsStyleName: (
    style: "soft" | "minimal" | "framed" | "accent",
  ) =>
    ({
      soft: "Dezente Karte",
      minimal: "Minimalistisch",
      framed: "Gerahmt",
      accent: "Alo-Akzent",
    })[style],
  quoteStudioAmountDetails: "Betragsdetails",
  quoteStudioTotalOnly: "Nur Gesamtsumme",
  quoteStudioTotalOnlyHelp: "Die kürzeste Übersicht",
  quoteStudioNetVatTotal: "Netto, MwSt. und Gesamt",
  quoteStudioNetVatTotalHelp: "Für die meisten Angebote empfohlen",
  quoteStudioVatBreakdown: "MwSt.-Aufschlüsselung",
  quoteStudioVatBreakdownHelp: "Alle MwSt.-Sätze anzeigen",
  quoteStudioCurrencyCode: "Währungscode",
  quoteStudioCurrencyCodeHelp: "EUR, USD oder die Angebotswährung anzeigen",
  quoteStudioEmphasizeTotal: "Gesamtsumme hervorheben",
  quoteStudioEmphasizeTotalHelp: "Den Endbetrag stärker gewichten",
  quoteStudioVatNote: "MwSt.-Hinweis",
  quoteStudioVatNoteHelp: "Erläutern, dass die MwSt. getrennt ausgewiesen wird",
  quoteStudioListItemFormatting: "Formatierung des Listeneintrags",
  quoteStudioDraftQuotation: "Angebotsentwurf",
  billingCustomizeInvoice: "Rechnung anpassen",
  billingInvoiceLabel: "Rechnung",
  billingInvoiceEdit: "Rechnung bearbeiten",
  billingInvoicePreview: "Rechnungsvorschau",
  billingQuoteQrLabel: "Scannen, um dieses Angebot anzunehmen",
  billingQuoteQrSubject: (number: string) => `Annahme des Angebots ${number}`,
  billingQuoteQrBody: (number: string) =>
    `Ich bestätige, dass ich das Angebot ${number} annehme. Bitte kontaktieren Sie mich, falls Sie weitere Informationen benötigen.`,
  billingInvoiceQrLabel: "Zum Bezahlen mit der Banking-App scannen",
  quoteStudioPricingTableNumber: (number: number) => `Preistabelle ${number}`,
  quoteStudioNumberedListColumns: "Spalten der nummerierten Liste",
  quoteStudioBulletListColumns: "Spalten der Aufzählung",
  quoteStudioParagraphColumns: "Spalten des Absatzes",
  quoteStudioQuoteColumns: "Spalten des Zitats",
  billingDownloadPdf: "PDF herunterladen",
  billingDownloadPdfFailed: "Die PDF konnte nicht heruntergeladen werden.",
  quoteStudioListStyle: "Listenstil",
  quoteStudioNumberingStyle: "Nummerierungsstil",
  quoteStudioBulletStyle: "Aufzählungsstil",
  quoteStudioChooseListStyle: "Wählen Sie einen Listenstil",
  quoteStudioIndentItem: "Eintrag einrücken",
  quoteStudioOutdentItem: "Einrückung verringern",
  quoteStudioListStyleName: (style: string) =>
    ({
      decimal: "Zahlen, Buchstaben, römisch",
      parenthesis: "Zahlen mit Klammern",
      outline: "Gliederung (1.1, 1.2.1)",
      "upper-alpha": "Großbuchstaben",
      roman: "Römische Ziffern",
      "leading-zero": "Führende Nullen (01, 02)",
      disc: "Runde Aufzählungszeichen",
      diamond: "Rauten und Pfeile",
      square: "Quadrate",
      arrow: "Pfeile",
      star: "Sterne",
      chevron: "Winkelpfeile",
      checkbox: "Kontrollkästchen",
    })[style] ?? style,
  quoteStudioColumnCount: (count: number) =>
    count === 1 ? "1 Spalte" : `${count} Spalten`,
  quoteStudioNumberedItemA11y: (number: number) =>
    `Nummerierter Eintrag ${number}`,
  quoteStudioBulletItemA11y: (number: number) => `Aufzählungseintrag ${number}`,
  quoteStudioBelowImage: "Unter dem Bild",
  quoteStudioImageLeft: "Bild links",
  quoteStudioImageRight: "Bild rechts",
  quoteStudioNatural: "Natürlich",
  quoteStudioWide: "Breit",
  quoteStudioSquare: "Quadratisch",
  quoteStudioFillFrame: "Rahmen füllen",
  quoteStudioWholeImage: "Ganzes Bild",
  quoteStudioColumnNumber: (number: number) => `Spalte ${number}`,
  quoteStudioColumnNameA11y: (number: number) => `Name der Spalte ${number}`,
  quoteStudioRemoveColumnA11y: (label: string) => `${label} entfernen`,
  quoteStudioRemoveRowA11y: (row: number) => `Zeile ${row} entfernen`,
  quoteStudioTableCellA11y: (column: string, row: number) =>
    `${column}, Zeile ${row}`,
  quoteStudioCategoryMedia: "Medien",
  quoteStudioCategoryTables: "Tabellen",
  quoteStudioCategoryLayout: "Layout",
  quoteStudioSearchResults: "Suchergebnisse",
  quoteStudioDesignDatabaseError:
    "Die Datenbank für Angebotsgestaltungen konnte nicht geöffnet werden.",
  quoteStudioDesignSaveError:
    "Die Angebotsgestaltung konnte nicht gespeichert werden.",
  quoteStudioDesignSaveCancelled:
    "Das Speichern der Angebotsgestaltung wurde abgebrochen.",
  quoteStudioDesignSaveRetry:
    "Die Angebotsgestaltung konnte nicht gespeichert werden. Versuchen Sie es mit einem kleineren Bild oder laden Sie es erneut hoch.",
  quoteStudioShowSubtotal: "Zwischensumme anzeigen",
  quoteStudioHideSubtotal: "Zwischensumme ausblenden",
  quoteStudioQuotationImageAlt: "Bild im Angebot",
  quoteStudioNoProposalContent: "Noch kein Angebotsinhalt",
  quoteStudioStartQuotationBelow: "Beginnen Sie unten mit Ihrem Angebot",
  billingExitPreview: "Vorschau schließen",
  quoteStudioModern: "Modern",
  quoteStudioModernHelp: "Klar und souverän",
  quoteStudioEditorial: "Editorial",
  quoteStudioEditorialHelp: "Überschriften mit erzählerischem Charakter",
  quoteStudioMinimal: "Minimal",
  quoteStudioMinimalHelp: "Ruhig und präzise",
  quoteStudioSignature: "Signatur",
  quoteStudioSignatureHelp:
    "Ausgewogenes Verhältnis von Identität und Angebotsdaten",
  quoteStudioHeaderEditorialHelp: "Ein selbstbewusster, titelbetonter Einstieg",
  quoteStudioBrandBand: "Markenband",
  quoteStudioBrandBandHelp: "Eine markantere Einführung in die Marke",
  quoteStudioHeaderMinimalHelp: "Ruhig, kompakt und präzise",
  quoteStudioLogoStack: "Logo über Name",
  quoteStudioLogoStackHelp: "Unternehmensname unter dem Logo",
  billingVatIncludedNote: "Die Umsatzsteuer ist im Gesamtbetrag enthalten.",
  billingVatSeparateNote:
    "Die Umsatzsteuer wird getrennt vom Nettobetrag ausgewiesen.",
  billingPricingTableEditorHelp:
    "Produkte und Leistungen hinzufügen, bearbeiten, entfernen oder in die richtige Reihenfolge ziehen.",
  billingPricingTableEmptyHelp:
    "Fügen Sie ein Produkt oder eine Leistung hinzu, um diese Preistabelle zu beginnen.",
  billingImage: "Bild",
  billingQuoteExitPreviewToEdit:
    "Vorschau verlassen, um dieses Angebot zu bearbeiten",
  billingQuoteEditContent: "Angebotsinhalt bearbeiten",
  billingQuoteCreateRevision:
    "Revision erstellen, um dieses finalisierte Angebot zu bearbeiten",
  billingQuoteEdit: "Angebot bearbeiten",
  billingQuoteExitPreviewToCustomize:
    "Vorschau verlassen, um dieses Angebot anzupassen",
  billingQuoteCreateRevisionToCustomize:
    "Revision erstellen, um dieses finalisierte Angebot anzupassen",
  billingQuoteCreateRevisionTitle: "Eine bearbeitbare Revision erstellen?",
  billingQuoteCreateRevisionConfirm:
    "Das finalisierte Angebot bleibt unverändert. alo erstellt einen neuen Entwurf mit demselben Kunden, Inhalt, Preisen und Design.",
  billingQuoteCreateRevisionAction: "Revision erstellen",
  billingConnectionsProductCount: (count: number) =>
    count === 1 ? "1 Produkt" : `${count} Produkte`,
  billingConnectionsUpdateCadence: (cadence: string) =>
    `Aktualisierung: ${cadence}`,
  billingConnectionsViaAlo: "Über alo verbunden",
  billingConnectionsExternalApi: "Externe API",
  billingConnectionsReviewChanges: (count: number) =>
    `${count} Änderungen prüfen`,
  billingConnectionsResume: "Fortsetzen",
  billingConnectionsPause: "Pausieren",
  billingConnectionsDisconnectCompany: (company: string) =>
    `${company} trennen`,
  billingConnectionsSpreadsheetFeed: "Tabelle oder Feed",
  billingConnectionsPriceApiAddress: "Adresse der Preis-API",
  billingConnectionsFeedAddress: "Feed-Adresse",
  billingConnectionsFormatDetection:
    "alo erkennt JSON-, CSV- und Tabellen-Feeds automatisch.",
  billingConnectionsAddressPlaceholder: "https://lieferant.beispiel/preise",
  billingConnectionsAdvancedSettings: "Erweiterte Einstellungen",
  billingConnectionsEveryHour: "Stündlich",
  billingConnectionsOnceDay: "Einmal täglich",
  billingConnectionsOnceWeek: "Einmal wöchentlich",
  billingConnectionsManualSync: "Nur bei manueller Synchronisierung",
  billingConnectionsReviewEveryChange: "Jede Änderung prüfen",
  billingConnectionsAutomaticLimited:
    "Innerhalb eines Grenzwerts automatisch übernehmen",
  billingConnectionsAutomaticAll: "Alle Änderungen automatisch übernehmen",
  billingConnectionsMatchSku: "Artikelnummer, dann Barcode und Name",
  billingConnectionsMatchBarcode: "Barcode, dann Artikelnummer und Name",
  billingConnectionsMatchName: "Produktname",
  billingConnectionsMatchReview: "Jede Zuordnung prüfen",
  billingConnectionsHoldReview: "Zur Prüfung zurückhalten",
  billingConnectionsCreateDraftItems:
    "Preislistenpositionen als Entwurf anlegen",
  billingConnectionsDoNotImport: "Nicht importieren",
  billingConnectionsHeaderNamePlaceholder: "X-API-Key",
  billingConnectionsAloInvitationLink: "alo-Einladungslink",
  billingConnectionsExternalPricingApi: "Externe Preis-API",
  billingConnectionsTestSummary: (
    found: number,
    matched: number,
    review: number,
  ) =>
    `${found} Produkte gefunden · ${matched} automatisch zugeordnet · ${review} nach dem Verbinden zu prüfen.`,
  billingConnectionsCustomHeaderHelp:
    "Optional. Nur verwenden, wenn die Dokumentation des Lieferanten einen anderen Header als den obigen Zugangsschlüssel verlangt.",
  billingConnectionsInviteAloWorkspace: "alo-Arbeitsbereich einladen",
  billingConnectionsGiveExternalApi: "Externen API-Zugriff gewähren",
  billingConnectionsLivePriceListActive: (count: number) =>
    `Live-Preisliste · ${count} aktive Produkte`,
  billingConnectionsChooseProductsSelected: (count: number) =>
    `Produkte auswählen · ${count} ausgewählt`,
  billingConnectionsItemUnit: "Stück",
  billingConnectionsPrices: "Preise",
  billingConnectionsUpdates: "Aktualisierungen",
  billingConnectionsValidity: "Gültigkeit",
  billingConnectionsLivePriceListCount: (count: number) =>
    `Live-Preisliste (${count})`,
  billingConnectionsSelectedProductsCount: (count: number) =>
    `${count} ausgewählte Produkte`,
  billingConnectionsChangesFlow:
    "Preislistenänderungen laufen über diese Verbindung",
  billingConnectionsNoExpiry: "Unbefristet",
};
