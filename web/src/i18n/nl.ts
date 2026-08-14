// Dutch (Nederlands) catalog. Typed as `Partial<Catalog>`: any key not
// present here falls back to English, so this can grow incrementally
// without ever showing a blank label. Register: formal (u/uw), suited to
// Belgian/Flemish business users; standard Dutch (read natively in
// Flanders). Brand nouns (alo, Chat, Meet, Drive) stay as-is.
import type { Catalog } from "./en";

export const nl: Partial<Catalog> = {
  moduleSites: "Websites",
  sitesAddFirstSection: "Voeg je eerste sectie toe",
  sitesAddressAvailable: "Beschikbaar",
  sitesAddressTaken: "Al in gebruik",
  sitesAddressNotChecked:
    "Voer een geldig adres in om de beschikbaarheid te controleren",
  sitesNameRequired: "Geef je website een naam om door te gaan.",
  sitesAddressRequired: "Voer een websiteadres in om door te gaan.",
  driveEmpty: "Deze map is leeg. Upload een bestand of maak een map.",
  driveEmptyTitle: "Hier staat nog niets",
  driveEmptyReadOnly: "Deze ruimte bevat nog geen bestanden.",
  driveEmptyTrashTitle: "De prullenbak is leeg",
  driveEmptyTrash: "Verwijderde items verschijnen hier.",
  driveFolderEmpty: "Deze map is leeg",
  driveUploadHere: "Hier uploaden",
  driveSort: "Sorteren",
  driveSortNameAsc: "Naam (A–Z)",
  driveSortNameDesc: "Naam (Z–A)",
  driveSortNewest: "Nieuwste eerst",
  driveSortOldest: "Oudste eerst",
  driveSortLargest: "Grootste eerst",
  driveSortSmallest: "Kleinste eerst",
  driveView: "Weergave",
  driveViewExtraLarge: "Extra grote pictogrammen",
  driveViewLarge: "Grote pictogrammen",
  driveViewMedium: "Middelgrote pictogrammen",
  driveViewSmall: "Kleine pictogrammen",
  driveViewList: "Lijst",
  driveViewDetails: "Details",
  driveViewTiles: "Tegels",
  driveViewContent: "Inhoud",
  driveViewNavigationPane: "Navigatievenster",
  driveViewCompact: "Compacte weergave",
  driveViewExtensions: "Bestandsnaamextensies",
  taskCreating: "Aanmaken…",
  taskFilesAttachTo: "Toevoegen aan taak",
  taskFilesDropHint:
    "Sleep afbeeldingen of bestanden hierheen, of kies Bijlage toevoegen.",
  taskFilesNeedTask:
    "Maak eerst een taak en voeg er daarna afbeeldingen en bestanden aan toe.",
  taskFilesUploadError:
    "Deze bestanden konden niet worden toegevoegd. Probeer opnieuw.",
  taskChooseFromDrive: "Kiezen uit Drive",
  taskChooseFromDriveHint:
    "Voeg bestaande bestanden toe zonder ze opnieuw te uploaden.",
  taskSearchDrive: "Zoeken in deze map",
  taskDriveBack: "Terug naar de vorige map",
  taskNoDriveFiles: "Geen bestanden in deze map.",
  taskAttachSelected: "Selectie toevoegen",
  taskFilesSelected: (count: number) =>
    count === 1 ? "1 bestand geselecteerd" : `${count} bestanden geselecteerd`,
  taskCreateOnDate: (date: string) => `Taak maken met vervaldatum ${date}`,
  // brand
  appName: "alo",
  tagline: "De soevereine, AI-native werkomgeving voor Europa.",

  // modules
  moduleHome: "Start",
  moduleMail: "E-mail",
  moduleAgenda: "Agenda",
  moduleChat: "Chat",
  moduleMeet: "Meet",
  moduleDrive: "Drive",
  moduleDocs: "Documenten",

  // Search
  moduleSearch: "Zoeken",
  searchPlaceholder: "Zoek bestanden, taken en e-mail…",
  searchHint: "Zoek bestanden en taken op naam, en e-mail op inhoud.",
  searchNoResults: "Niets gevonden.",
  aiAskAbout: (q: string): string => `Vraag het de AI: “${q}”`,
  aiSources: "Bronnen",
  aiUnconfigured:
    "AI is nog niet ingesteld — een beheerder kan een model toevoegen. Dit kwam overeen:",
  aiUnreachable: "De AI was niet bereikbaar. Dit kwam overeen:",
  searchKind: (kind: string) =>
    kind === "task"
      ? "Taak"
      : kind === "message"
        ? "E-mail"
        : kind === "folder"
          ? "Map"
          : kind === "doc"
            ? "Document"
            : kind === "base"
              ? "Base"
              : "Bestand",

  // Home dashboard
  homeGreetingMorning: "Goedemorgen",
  homeGreetingAfternoon: "Goedemiddag",
  homeGreetingEvening: "Goedenavond",
  homeWelcome: "Welkom bij alo workplace",
  homeStatUnreadEmails: "Ongelezen e-mails",
  homeStatEvents: "Aankomende afspraken",
  homeStatMessages: "Ongelezen berichten",
  homeStatFiles: "Documenten",
  homeGoToMail: "Naar E-mail",
  homeViewAgenda: "Agenda openen",
  homeOpenChat: "Chat openen",
  homeOpenDrive: "Drive openen",
  homeComingSoonShort: "Binnenkort",
  homeRecent: "Recent",
  homeStarred: "Met ster",
  homeUnread: "Ongelezen",
  homeViewAll: "Alles bekijken",
  homeNoRecent: "Nog niets hier.",
  homeQuickActions: "Snelle acties",
  homeCompose: "E-mail opstellen",
  homeCreateEvent: "Afspraak maken",
  homeStartChat: "Chat starten",
  homeUploadFile: "Bestand uploaden",
  homeCreateDoc: "Document maken",
  homeToday: "Vandaag",
  homeAgendaComingSoon:
    "Uw agenda verschijnt hier zodra de kalender beschikbaar is.",
  homeAskTitle: "Vraag alo alles",
  homeAskBody: "Uw AI-assistent voor al uw werk.",
  homeAskCta: "Vraag alo",
  moduleAi: "Vraag AI",

  // shell
  newButton: "Nieuw",
  appLauncher: "Apps",
  appLauncherFavorites: "Je favorieten",
  appLauncherAll: "Alle apps",
  appLauncherEdit: "Favorieten bewerken",
  appLauncherDone: "Klaar",
  appLauncherCancel: "Annuleren",
  appLauncherDragHint: "Sleep je zes favoriete apps naar de gewenste plek",
  appLauncherAddFavorite: "Toevoegen aan favorieten",
  appLauncherRemoveFavorite: "Verwijderen uit favorieten",
  userMenu: "Account",
  language: "Taal",
  signOut: "Afmelden",

  // contacts (address book)
  contactsTitle: "Contacten",
  contactsOpen: "Contacten",
  contactsSearchPlaceholder: "Contacten zoeken…",
  contactsEmpty: "Nog geen contacten. Voeg uw eerste toe.",
  contactsSearchEmpty: "Geen contacten gevonden voor uw zoekopdracht.",
  contactsLoadError: "Kon uw contacten niet laden.",
  contactsNew: "Nieuw contact",
  contactEdit: "Contact bewerken",
  contactFirstName: "Voornaam",
  contactLastName: "Achternaam",
  contactDisplayName: "Weergavenaam",
  contactEmail: "E-mail",
  contactPhone: "Telefoon",
  contactOrganization: "Organisatie",
  contactJobTitle: "Functie",
  contactNotes: "Notities",
  contactAddEmail: "E-mail toevoegen",
  contactAddPhone: "Telefoon toevoegen",
  contactRemoveField: "Verwijderen",
  contactKindWork: "Werk",
  contactKindHome: "Privé",
  contactKindMobile: "Mobiel",
  contactKindOther: "Overig",
  contactSave: "Opslaan",
  contactCancel: "Annuleren",
  contactDelete: "Verwijderen",
  contactDeleteConfirm: (name: string) =>
    `${name} verwijderen? Dit kan niet ongedaan worden gemaakt.`,
  contactNeedsName: "Voeg een naam of ten minste één e-mailadres toe.",
  contactSaveError: "Kon dit contact niet opslaan.",
  contactDeleteError: "Kon dit contact niet verwijderen.",
  contactNoEmail: "Geen e-mail",
  contactsImport: "Importeren",
  contactsExport: "Exporteren",
  contactsImporting: "Importeren…",
  contactsImported: (n: number, skipped: number) =>
    skipped > 0
      ? `${n} contact${n === 1 ? "" : "en"} geïmporteerd (${skipped} overgeslagen).`
      : `${n} contact${n === 1 ? "" : "en"} geïmporteerd.`,
  contactsImportError:
    "Kon dat bestand niet importeren. Is het een .vcf-export?",
  contactsExportError: "Kon uw contacten niet exporteren.",
  contactsExportEmpty: "U hebt nog geen contacten om te exporteren.",

  // import mail (IMAP wizard)
  importOpen: "E-mail importeren",
  importTitle: "E-mail importeren uit een ander account",
  importIntro:
    "Haal uw recente e-mail uit Gmail, Outlook of elk IMAP-account naar uw postvak.",
  importProvider: "Waar staat uw e-mail?",
  importProviderGmail: "Gmail",
  importProviderOutlook: "Outlook",
  importProviderOther: "Overig (IMAP)",
  importServer: "Mailserver",
  importPort: "Poort",
  importEmail: "E-mailadres",
  importPassword: "Wachtwoord",
  importAppPasswordHint:
    "Voor Gmail en Outlook hebt u een app-wachtwoord nodig, niet uw gewone wachtwoord.",
  importStart: "Import starten",
  importRunning: "Uw e-mail wordt geïmporteerd — dit kan een minuut duren…",
  importDone: (imported: number, skipped: number) =>
    skipped > 0
      ? `${imported} bericht${imported === 1 ? "" : "en"} geïmporteerd (${skipped} al aanwezig).`
      : `${imported} bericht${imported === 1 ? "" : "en"} geïmporteerd.`,
  importNeedsFields: "Vul de server, uw e-mailadres en uw wachtwoord in.",
  importClose: "Sluiten",
  signedInAs: "Aangemeld als",
  comingSoonTitle: "Binnenkort",
  comingSoonBody:
    "Dit deel van uw werkomgeving komt eraan. E-mail is nu al klaar.",

  // auth — brand panel
  brandHeadline: "Uw werkomgeving.\nUw servers.\nUw regels.",
  brandSubtitle:
    "E-mail, agenda, chat en bestanden — soeverein, AI-native en gehost in Europa.",
  brandEuBadge: "Gehost op uw infrastructuur · EU",
  brandHeadlineMail: "Uw e-mail.\nUw privacy.\nUw regels.",
  brandSubtitleMail: "Privé, AI-native e-mail — soeverein en gehost in Europa.",
  brandEuBadgeMail: "Soevereine e-mail · Gehost in Europa",

  // auth — sign in
  signInHeading: "Aanmelden",
  signInSubtitle: "Welkom terug. Voer uw gegevens in om verder te gaan.",
  emailLabel: "E-mailadres",
  emailPlaceholder: "u@uwdomein.com",
  emailPlaceholderMail: "u@alomails.com",
  passwordLabel: "Wachtwoord",
  showPassword: "Wachtwoord tonen",
  hidePassword: "Wachtwoord verbergen",
  rememberMe: "Aangemeld blijven",
  forgotPassword: "Wachtwoord vergeten?",
  forgotPasswordNote:
    "Neem contact op met uw beheerder om uw wachtwoord te herstellen.",
  signInButton: "Aanmelden",
  signingIn: "Bezig met aanmelden…",
  orDivider: "of",
  signInWithSso: "Aanmelden met SSO",
  ssoComingSoon: "Eenmalige aanmelding (SSO) komt binnenkort.",

  // auth — two-factor
  twoFactorTitle: "Tweefactorauthenticatie",
  twoFactorSubtitle: "Voer de zescijferige code uit uw authenticator-app in",
  twoFactorRecoverySubtitle: "Voer een van uw herstelcodes in",
  twoFactorCodeLabel: "Authenticatiecode",
  recoveryCodeLabel: "Herstelcode",
  recoveryPlaceholder: "xxxx-xxxx",
  verify: "Verifiëren",
  verifying: "Verifiëren…",
  useRecoveryCode: "Gebruik in plaats daarvan een herstelcode",
  useAuthenticator: "Gebruik in plaats daarvan uw authenticator-app",
  backToSignIn: "Terug naar aanmelden",

  // auth — errors
  errorBadCredentials:
    "Dit e-mailadres of wachtwoord klopt niet. Probeer het opnieuw.",
  errorSecondFactor: "Voer uw authenticatiecode in om verder te gaan.",
  errorBadOtp: "Deze code klopt niet. Probeer het opnieuw.",
  errorRateLimited: "Te veel pogingen. Wacht even en probeer het opnieuw.",
  errorGeneric: "Er ging iets mis bij het aanmelden. Probeer het opnieuw.",
  errorNetwork:
    "Kan de server niet bereiken. Controleer uw verbinding en probeer opnieuw.",
  signingOut: "Bezig met afmelden…",

  // signup — personal accounts
  signupHeading: "Maak uw persoonlijke alo-adres",
  signupSubtitle: "Privé, soevereine e-mail — nooit reclame, nooit tracking.",
  signupAddressLabel: "Kies uw adres",
  signupPickPlaceholder: "uwnaam",
  signupRecoveryLabel: "Uw huidige e-mailadres",
  signupRecoveryHint:
    "We sturen hier een verificatiecode naartoe — dit wordt ook uw herstel-adres.",
  signupSendCode: "Verificatiecode versturen",
  signupSending: "Versturen…",
  signupChecking: "Controleren…",
  signupAvailable: "Dit adres is beschikbaar",
  signupTaken: "Dit adres is al in gebruik",
  signupReserved: "Dit adres is gereserveerd",
  signupInvalid: "Gebruik 3–64 letters, cijfers, punten of streepjes",
  signupVerifyHeading: "Voer uw code in",
  signupVerifySubtitle: (recovery: string) =>
    `We stuurden een zescijferige code naar ${recovery}. Deze verloopt over 10 minuten.`,
  signupCodeLabel: "Verificatiecode",
  signupPasswordLabel: "Kies een wachtwoord",
  signupPasswordHint: "Minstens 8 tekens.",
  signupCreate: "Account aanmaken",
  signupCreating: "Uw account wordt aangemaakt…",
  signupResend: "Code opnieuw versturen",
  signupVerifyError: "Deze code is onjuist of verlopen. Probeer het opnieuw.",
  signupBeginError: "We konden de code niet versturen. Probeer het opnieuw.",
  signupDoneHeading: "Alles is klaar",
  signupDoneBody: (email: string) =>
    `${email} is klaar. Meld u aan met uw nieuwe adres en wachtwoord.`,
  signupGoToLogin: "Naar aanmelden",
  signupUnavailable: "Persoonlijke registraties zijn momenteel niet open.",
  signupHaveAccount: "Hebt u al een account?",
  signupBackToLogin: "Aanmelden",
  signupCreateLink: "Een persoonlijk account maken",

  // auth — password reset
  resetHeading: "Uw wachtwoord herstellen",
  resetSubtitle:
    "Voer uw alo-adres in — we mailen een herstelcode naar uw herstel-postvak.",
  resetAddressLabel: "Uw alo-adres",
  resetSendCode: "Herstelcode versturen",
  resetSending: "Versturen…",
  resetVerifyHeading: "Voer de code in",
  resetVerifySubtitle: (address: string) =>
    `Als ${address} een alo-account heeft, is er een herstelcode onderweg naar het herstel-postvak. Voer die hieronder in met een nieuw wachtwoord.`,
  resetNewPasswordLabel: "Nieuw wachtwoord",
  resetSubmit: "Nieuw wachtwoord instellen",
  resetSubmitting: "Opslaan…",
  resetDoneHeading: "Wachtwoord bijgewerkt",
  resetDoneBody: "U kunt zich nu aanmelden met uw nieuwe wachtwoord.",
  resetRequestError: "We konden het herstel niet starten. Probeer het opnieuw.",
  resetVerifyError: "Dat lukte niet — controleer de code en probeer opnieuw.",

  // agenda (calendar)
  agendaNewEvent: "Nieuwe afspraak",
  agendaCalendars: "Kalenders",
  agendaCalendar: "Kalender",
  agendaNewCalendar: "Nieuwe kalender",
  agendaNewCalendarPrompt: "Naam voor de nieuwe kalender",
  agendaDeleteCalendar: "Kalender verwijderen",
  agendaToday: "Vandaag",
  agendaPrev: "Vorige",
  agendaToolbarLabel: "Agenda",
  agendaViewLabel: "Weergave",
  agendaNext: "Volgende",
  agendaMonth: "Maand",
  agendaWeek: "Week",
  agendaAllDay: "Hele dag",
  agendaEventTitle: "Voeg een titel toe",
  agendaEventStart: "Begint",
  agendaEventEnd: "Eindigt",
  agendaEventLocation: "Locatie",
  rsvpFrom: "Van",
  rsvpAccept: "Accepteren",
  rsvpMaybe: "Misschien",
  rsvpDecline: "Afwijzen",
  rsvpAccepted: "U hebt deze uitnodiging geaccepteerd.",
  rsvpDeclined: "U hebt deze uitnodiging afgewezen.",
  rsvpTentative: "U hebt Misschien geantwoord.",
  replyResponded: "reageerde",
  replyFrom: (who: string, verb: string) => `${who} ${verb}`,
  replyApplied: "Bijgewerkt op uw afspraak.",
  rsvpError: "Kon uw antwoord niet versturen — probeer het opnieuw.",
  cancelledTitle: "Geannuleerd:",
  cancelledRemoved: "Verwijderd uit uw agenda.",
  cancelledAbsent: "Deze afspraak stond niet in uw agenda.",
  agendaEventGuests: "Genodigden",
  agendaGuestsPlaceholder: "naam@voorbeeld.com, andere@voorbeeld.com",
  agendaGuestsHint:
    "We mailen elke genodigde een uitnodiging die ze in hun eigen agenda kunnen accepteren.",
  agendaEventDescription: "Notities",
  agendaSave: "Opslaan",
  agendaSaveThis: "Deze afspraak",
  agendaSaveAll: "Hele reeks",
  agendaDelete: "Verwijderen",
  agendaDeleteThis: "Deze afspraak",
  agendaDeleteAll: "Hele reeks",
  agendaCancel: "Annuleren",
  agendaNewEventTitle: "Nieuwe afspraak",
  agendaEditEventTitle: "Afspraak bewerken",
  agendaEndBeforeStart: "De afspraak eindigt vóór het begin.",
  agendaSaveError: "Kon de afspraak niet opslaan. Probeer het opnieuw.",
  agendaRepeat: "Herhalen",
  agendaRepeatNone: "Niet herhalen",
  agendaRepeatDaily: "Elke dag",
  agendaRepeatWeekly: "Elke week",
  agendaRepeatWeekdays: "Elke werkdag (ma–vr)",
  agendaRepeatMonthly: "Elke maand",
  agendaRepeatYearly: "Elk jaar",
  // taken
  moduleTasks: "Taken",
  taskProjects: "Projecten",
  taskNewProject: "Nieuw project",
  taskNewProjectPrompt: "Naam voor het nieuwe project",
  taskMyPlate: "Mijn taken",
  taskProposals: "Suggesties",
  taskBoard: "Bord",
  taskList: "Lijst",
  taskQuickAdd: "Een taak toevoegen…",
  taskAdd: "Toevoegen",
  taskColTodo: "Te doen",
  taskColInProgress: "Bezig",
  taskColDone: "Klaar",
  taskDueToday: "Vandaag",
  taskDueTomorrow: "Morgen",
  taskDueYesterday: "Gisteren",
  taskPrioNone: "Geen",
  taskPrioLow: "Laag",
  taskPrioMedium: "Gemiddeld",
  taskPrioHigh: "Hoog",
  taskFromEmail: "Uit een e-mail",
  taskFromEvent: "Uit een afspraak",
  taskOpenEmail: "Open de bron-e-mail",
  createTask: "Taak aanmaken",
  suggestTasks: "Taken voorstellen uit deze e-mail",
  taskCreatedFromMail: "Taak aangemaakt uit deze e-mail.",
  taskSuggesting: "E-mail lezen op actiepunten…",
  taskNoSuggestions: "Geen actiepunten gevonden in deze e-mail.",
  taskSuggested: (n: number) =>
    n === 1
      ? "1 suggestie toegevoegd aan je takenpostvak."
      : `${n} suggesties toegevoegd aan je takenpostvak.`,
  taskAiOff: "AI staat uit, er kon niets worden voorgesteld.",
  taskClose: "Sluiten",
  taskDelete: "Verwijderen",
  taskAssignee: "Toegewezen aan",
  taskAssigneePlaceholder: "naam@voorbeeld.com",
  taskDue: "Vervaldatum",
  taskPriority: "Prioriteit",
  taskDescription: "Omschrijving",
  taskDescriptionPlaceholder: "Meer details toevoegen…",
  taskSubtasks: "Subtaken",
  taskAddSubtask: "Een subtaak toevoegen…",
  taskComments: "Reacties",
  taskAddComment: "Schrijf een reactie…",
  taskActivity: "Activiteit",
  taskEmpty: "Nog geen taken. Voeg er hierboven een toe.",
  taskPlateEmpty: "Niets te doen. U bent helemaal bij.",
  taskNoProposalsTitle: "U bent helemaal bij",
  taskNoProposals:
    "Suggesties verschijnen hier wanneer alo actiepunten in een e-mail vindt.",
  taskAiSuggested: "Voorgesteld door AI",
  taskAccept: "Accepteren",
  taskReject: "Negeren",
  taskActivityKind: (kind: string) =>
    (
      ({
        created: "heeft deze taak gemaakt",
        status_changed: "heeft ze verplaatst",
        assigned: "heeft de toegewezene gewijzigd",
        due_changed: "heeft de vervaldatum gewijzigd",
        commented: "heeft gereageerd",
        accepted: "heeft de suggestie geaccepteerd",
        proposed: "is voorgesteld door AI",
      }) as Record<string, string>
    )[kind] ?? kind,
  agendaReminder: "Herinnering",
  agendaReminderNone: "Geen herinnering",
  agendaReminderAtStart: "Op het tijdstip van de afspraak",
  agendaReminder5: "5 minuten ervoor",
  agendaReminder10: "10 minuten ervoor",
  agendaReminder15: "15 minuten ervoor",
  agendaReminder30: "30 minuten ervoor",
  agendaReminder60: "1 uur ervoor",
  agendaReminder1Day: "1 dag ervoor",
  agendaRsvpAccepted: "Geaccepteerd",
  agendaRsvpDeclined: "Afgewezen",
  agendaRsvpTentative: "Misschien",
  agendaRsvpPending: "Nog geen antwoord",
  agendaCheckAvailability: "Beschikbaarheid controleren",
  agendaAvailChecking: "Controleren…",
  agendaAvailAllFree: "Iedereen is dan vrij.",
  agendaAvailBusy: (names: string) => `Dan bezet: ${names}`,
  agendaAvailNoGuests:
    "Voeg genodigden toe om hun beschikbaarheid te controleren.",
  agendaAvailError: "Kon de beschikbaarheid niet controleren.",
  agendaClose: "Sluiten",
  agendaReadOnly: "U hebt alleen-lezen toegang tot deze kalender.",
  // Calendar sharing
  agendaShare: "Kalender delen",
  agendaShareTitle: (name: string) => `„${name}” delen`,
  agendaShareWith: "Delen met",
  agendaSharePerson: "Een persoon",
  agendaShareGroupOption: "Een groep",
  agendaShareEmail: "E-mailadres",
  agendaShareEmailPlaceholder: "naam@voorbeeld.com",
  agendaShareGroupPick: "Kies een groep…",
  agendaShareAccess: "Toegang",
  agendaShareViewer: "Kan bekijken",
  agendaShareEditor: "Kan bewerken",
  agendaShareGroup: "Groep",
  agendaShareAdd: "Delen",
  agendaShareRemove: "Verwijderen",
  agendaShareRemoveFor: (name: string) => `Niet meer delen met ${name}`,
  agendaShareEmpty: "Nog met niemand gedeeld.",
  agendaShareLoadError: "Kon niet laden met wie dit is gedeeld.",
  agendaShareError: "Kon het delen niet bijwerken. Probeer het opnieuw.",

  // mail
  mailLoading: "Uw e-mail wordt geladen…",
  mailSearching: "Zoeken…",
  mailFolders: "Mappen",
  flaggedView: "Gemarkeerd",
  flagDueAdd: "Vervaldatum toevoegen",
  flagDueToday: "Vandaag",
  flagDueTomorrow: "Morgen",
  flagDueNextWeek: "Volgende week",
  flagDuePick: "Kies een datum…",
  flagDueClear: "Vervaldatum wissen",
  flagDueLabel: (when: string) => `Te doen ${when}`,
  flagDueOverdue: (when: string) => `Te laat — moest klaar zijn ${when}`,
  flagDueSet: "Een opvolgdatum instellen",
  resizeFolders:
    "Grootte van het mappenpaneel wijzigen (sleep, of pijltoetsen; dubbelklik om te herstellen)",
  resizeMessages:
    "Grootte van de berichtenlijst wijzigen (sleep, of pijltoetsen; dubbelklik om te herstellen)",
  collapseFolders: "Mappen verbergen",
  expandFolders: "Mappen tonen",
  mailEmpty: "Nog geen berichten hier.",
  mailSearchEmpty: "Geen berichten gevonden voor uw zoekopdracht.",
  mailSelectPrompt: "Je inbox is klaar",
  mailSelectBody: "Kies een bericht in de lijst om het gesprek te openen.",
  mailListError: "Kon berichten niet laden.",
  mailFolderError: "Kon uw mappen niet laden.",
  mailRetry: "Opnieuw proberen",
  mailFrom: "Van",
  mailTo: "Aan",
  mailNoSubject: "(geen onderwerp)",
  mailUnknownSender: "Onbekende afzender",

  // mail — sidebar
  compose: "Opstellen",
  mailSearchPlaceholder: "E-mail zoeken…",
  viewAsMessages: "Als losse berichten tonen",
  viewAsConversations: "Als gesprekken tonen",

  // mail — reading pane
  conversationActions: "Acties voor dit gesprek",
  reply: "Beantwoorden",
  replyAll: "Allen beantwoorden",
  forward: "Doorsturen",
  archive: "Archiveren",
  snooze: "Sluimeren",
  flag: "Markeren",
  unflag: "Markering verwijderen",
  markRead: "Markeren als gelezen",
  markUnread: "Markeren als ongelezen",
  selectAll: "Alles selecteren",
  selectNone: "Selectie wissen",
  selectedCount: (n: number) =>
    n === 1 ? "1 geselecteerd" : `${n} geselecteerd`,
  snoozeUntil: "Sluimeren tot…",
  snoozeLaterToday: "Later vandaag",
  snoozeTomorrow: "Morgen",
  snoozeWeekend: "Dit weekend",
  snoozeNextWeek: "Volgende week",
  mailSnoozed: "Gesluimerd",
  delete: "Verwijderen",
  dialogConfirm: "Bevestigen",
  dialogCancel: "Annuleren",
  dialogOk: "OK",
  deletePermanently: "Definitief verwijderen",
  moveTo: "Naar map verplaatsen",
  moreActions: "Meer acties",
  mailMoved: "Bericht verplaatst.",
  mailDeleted: "Bericht verwijderd.",
  mailActionFailed: "Dat lukte niet — probeer het opnieuw.",
  endOfMessage: "Einde van het bericht",
  threadMessages: "berichten",
  aloSummary: "alo-samenvatting",
  summaryPending: "Dit gesprek wordt samengevat…",
  smartReplies: "Voorgestelde antwoorden",
  quickReplyHint: "Allen beantwoorden · Doorsturen hierboven",
  toLabel: "aan",
  ccLabel: "cc",
  bccLabel: "bcc",
  recipientsNone: "—",
  senderVerified: "Geverifieerd",
  senderVerifiedTitle: "Afzender geauthenticeerd — SPF, DKIM en DMARC geslaagd",
  replyTo: "Antwoorden aan",
  quickReplyTo: (name: string) => `Snel antwoorden aan ${name}`,
  replyToName: (name: string) => `Antwoorden aan ${name}…`,
  draftWithAi: "Opstellen met AI",
  attachments: "Bijlagen",
  attach: "Bestanden bijvoegen",
  attachmentUploading: "Uploaden…",
  attachmentDownloading: "Downloaden…",
  attachmentUploadFailed: "Kon dat bestand niet uploaden.",
  downloadAttachment: (name: string) => `${name} downloaden`,
  attachmentFailed: "Kon die bijlage niet downloaden.",

  // mail — compose
  composeTitle: "Nieuw bericht",
  composeReplyTitle: "Beantwoorden",
  composeForwardTitle: "Doorsturen",
  composeForwardPrefix: "Fwd: ",
  composeForwardedIntro: "---------- Doorgestuurd bericht ----------",
  composeLabelFrom: "Van:",
  composeLabelDate: "Datum:",
  composeLabelSubject: "Onderwerp:",
  composeLabelTo: "Aan:",
  composeReplyAllTitle: "Allen beantwoorden",
  composeFrom: "Van",
  composeTo: "Aan",
  composeCc: "Cc",
  composeBcc: "Bcc",
  composeSubject: "Onderwerp",
  composeRecipientsPlaceholder: "naam@voorbeeld.com, …",
  composeSubjectPlaceholder: "Onderwerp",
  composeBodyPlaceholder: "Schrijf uw bericht…",
  composeSend: "Verzenden",
  composeSending: "Verzenden…",
  composeDiscard: "Verwerpen",
  composeCcToggle: "Cc",
  composeNoRecipients: "Voeg ten minste één ontvanger toe.",
  composeSendError: "Kon uw bericht niet verzenden. Probeer het opnieuw.",
  composeSent: "Bericht verzonden.",
  composeUndoWindow: "Verzenden…",
  composeUndoSend: "Ongedaan maken",
  composeSendUndone:
    "Verzenden ongedaan gemaakt — uw bericht staat in Concepten.",
  scheduleSend: "Verzenden plannen",
  scheduleTomorrowMorning: "Morgenochtend",
  scheduleTomorrowAfternoon: "Morgenmiddag",
  scheduleMondayMorning: "Maandagochtend",
  schedulePickTime: "Kies datum en tijd",
  mailScheduled: (when: string) => `Verzending gepland voor ${when}.`,
  scheduleError: "Kon uw bericht niet plannen. Probeer het opnieuw.",
  cancelSend: "Verzenden annuleren",
  sendCancelled:
    "Geplande verzending geannuleerd — uw bericht staat weer in Concepten.",
  contactSuggestions: "Overeenkomende contacten",
  labelColor: "Labelkleur",
  labelColorHint: "rechtsklik om te kleuren",
  labelColorClear: "Geen kleur",
  folderNew: "Nieuwe map",
  folderNewSub: "Nieuwe submap",
  folderRename: "Naam wijzigen",
  folderDelete: "Map verwijderen",
  folderNamePlaceholder: "Mapnaam",
  folderDeleteConfirm: (name: string) =>
    `De map "${name}" verwijderen? De berichten worden niet verwijderd.`,
  folderActionFailed: "Die mapwijziging lukte niet — probeer het opnieuw.",
  folderActions: (name: string) => `Opties voor de map ${name}`,
  sharedMailboxLabel: "Postvak",
  sharedMailboxesHeading: "Gedeelde postvakken",
  sharedMyMailbox: "Mijn postvak",
  sharedReadOnly: "alleen-lezen",
  sharedNoSend:
    "U kunt niet verzenden vanuit dit gedeelde postvak — u kreeg geen verzendrecht.",
  settingsSharing: "Delen",
  settingsSharingHint:
    "Laat collega's uw postvak openen en beheren. Geef verzendrecht om ze ook namens u te laten verzenden.",
  sharingNone: "U hebt uw postvak met niemand gedeeld.",
  sharingEmailPlaceholder: "E-mailadres van collega",
  sharingAdd: "Delen",
  sharingAddError:
    "Kon niet delen — controleer of het adres een collega in uw organisatie is.",
  userShareAccess: "Gedeelde toegang",
  delegateTitle: (email: string) => `Wie ${email} kan openen`,
  delegateIntro:
    "Personen die u toevoegt, kunnen dit postvak openen en beheren. Sta verzenden toe om ze ook vanaf dit adres te laten verzenden.",
  delegatePeople: "Personen met toegang",
  delegateNone: "Nog niemand anders heeft toegang.",
  delegateAdd: "Persoon toevoegen",
  delegateReadOnly: "Alleen-lezen",
  delegateManage: "Kan beheren",
  delegateAccessLabel: "Toegangsniveau",
  delegateSendLabel: "Verzendrecht",
  delegateSendNone: "Kan niet verzenden",
  delegateSendAs: "Verzenden als",
  delegateSendOnBehalf: "Verzenden namens",
  delegateRemove: "Toegang verwijderen",
  delegateRemoveFor: (email: string) => `Toegang van ${email} intrekken`,
  delegateFoldersFor: (email: string) => `${email} tot mappen beperken`,
  delegateError: "Die toegangswijziging lukte niet — probeer het opnieuw.",
  delegateFoldersLabel: "Beperken tot mappen",
  delegateWholeMailbox: "Volledig postvak",
  delegateLimitFolders: "Toegang beperken tot specifieke mappen",
  delegateFoldersSave: "Mappen opslaan",
  delegateFoldersCancel: "Annuleren",
  categories: "Categorieën",
  categorize: "Categoriseren",
  categoryNew: "Nieuwe categorie",
  categoryRename: "Naam wijzigen",
  categoryDelete: "Categorie verwijderen",
  categoryNamePlaceholder: "Categorienaam",
  categoryNoneHint: "Nog geen categorieën — voeg er een toe via de zijbalk.",
  categoryDeleteConfirm: (name: string) =>
    `De categorie "${name}" verwijderen? Ze wordt van elk bericht met deze categorie verwijderd.`,
  categoryActionFailed:
    "Die categoriewijziging lukte niet — probeer het opnieuw.",
  categoryActions: (name: string) => `Opties voor de categorie ${name}`,
  categoryClearFilter: "Alle berichten tonen",
  transferLink: "koppeling",
  transferSharedFile: "📎 Gedeeld bestand",
  transferDownload: "Downloaden",
  transferExpires: (date: string) => `koppeling verloopt ${date}`,
  transferExpiryTitle:
    "Hoe lang koppelingen voor grote bestanden actief blijven",
  transferExpiryOption: (days: number) =>
    days === 1 ? "1 dag" : `${days} dagen`,
  blockSenderNamed: (email: string) => `${email} blokkeren`,
  senderBlocked: (email: string) =>
    `${email} geblokkeerd — hun e-mail gaat nu naar Ongewenst.`,
  settingsFilters: "Filters en regels",
  settingsFiltersHint:
    "Regels draaien op uw server zodra e-mail binnenkomt — ook als u offline bent. De eerste passende regel wordt toegepast.",
  filtersLoadError: "Kon uw filters niet laden.",
  filtersSaveError: "Kon uw filters niet opslaan. Probeer het opnieuw.",
  filterAddRule: "Een regel toevoegen",
  filterNamePlaceholder: "Regelnaam (optioneel)",
  filterWhen: "Wanneer een bericht binnenkomt en",
  filterDo: "Doe dit",
  filterMatchAll: "alles klopt",
  filterMatchAny: "iets klopt",
  filterOr: "of",
  filterFieldFrom: "Van",
  filterFieldTo: "Aan",
  filterFieldCc: "Cc",
  filterFieldSubject: "Onderwerp",
  filterOpContains: "bevat",
  filterOpIs: "is exact",
  filterValuePlaceholder: "waarde",
  filterAddCondition: "Voorwaarde toevoegen",
  filterRemoveCondition: "Voorwaarde verwijderen",
  filterConditionField: (n: number) => `Voorwaarde ${n}: veld`,
  filterConditionOp: (n: number) => `Voorwaarde ${n}: vergelijking`,
  filterConditionValue: (n: number) => `Voorwaarde ${n}: waarde`,
  filterRemoveConditionAt: (n: number) => `Voorwaarde ${n} verwijderen`,
  filterRuleEnabled: (rule: string) => `Regel actief: ${rule}`,
  filterFolderLabel: "Doelmap",
  filterActionFileInto: "Naar map verplaatsen",
  filterActionMarkRead: "Markeren als gelezen",
  filterActionStar: "Ster geven",
  filterActionDelete: "Verwijderen",
  filterSaveRule: "Regel opslaan",
  filterCancel: "Annuleren",
  filterDelete: "Regel verwijderen",
  filterNeedsCondition: "Voeg ten minste één voorwaarde met een waarde toe.",
  filterNeedsAction: "Kies ten minste één actie.",
  composeWroteOn: "schreef:",
  composeReplyPrefix: "Re: ",
  composeBack: "Terug",
  composeExpand: "Volledig scherm",
  composeCollapse: "Volledig scherm sluiten",
  composeMinimize: "Minimaliseren",
  composeRestore: "Herstellen",
  showQuoted: "Geciteerde tekst tonen",
  showOriginal: "Origineel tonen",
  downloadEml: ".eml downloaden",
  print: "Afdrukken",
  reportSpam: "Spam melden",
  notSpam: "Geen spam",
  spamBannerTitle: "Dit bericht staat in Spam",
  spamReasonDmarc: (domain: string) =>
    `We konden niet bevestigen dat het echt van ${domain} kwam — het faalde de DMARC-authenticatie, een veelvoorkomend teken van vervalsing.`,
  spamReasonDkim:
    "De cryptografische handtekening (DKIM) klopte niet, dus de afzender kon niet worden geverifieerd.",
  spamReasonSpf: (domain: string) =>
    `De server die het verzond, mag geen e-mail versturen voor ${domain} (SPF gefaald).`,
  spamReasonNone:
    "We ontdekten geen bezorgprobleem met dit bericht — het lijkt mogelijk op e-mail die u of een filterregel eerder als spam markeerde.",
  spamBannerHint:
    "Als dit geen spam is, verplaats het terug naar uw Postvak IN.",
  spamSenderFallback: "het domein van de afzender",
  unsubscribe: "Uitschrijven",
  unsubscribeConfirm: (sender: string) =>
    `Uitschrijven bij ${sender}? We vragen de afzender u geen e-mail meer te sturen.`,
  unsubscribed: "Uitgeschreven — de afzender is gevraagd te stoppen.",
  unsubscribeFailed:
    "Kon niet automatisch uitschrijven — probeer de koppeling in het bericht.",
  unsubscribeOpened: "De uitschrijfpagina is in een nieuw tabblad geopend.",
  forwardAsAttachment: "Doorsturen als bijlage",
  blockSender: "Afzender blokkeren",
  junkUnavailable: "Er is geen map Ongewenst om dit naartoe te verplaatsen.",
  hideQuoted: "Geciteerde tekst verbergen",
  formatting: "Tekstopmaak",
  bold: "Vet",
  italic: "Cursief",
  underline: "Onderstrepen",
  link: "Koppeling invoegen",
  linkPrompt: "URL van koppeling:",
  improve: "Verbeteren",
  aiImproveFailed: "De AI kon dat nu niet herschrijven.",

  // account settings
  settingsOpen: "Instellingen",
  settingsTitle: "E-mailinstellingen",
  settingsTabGeneral: "Algemeen",
  settingsTabOrg: "Organisatie",
  settingsOooToggle: "Automatische antwoorden versturen",
  settingsSignature: "Uw handtekening",
  settingsSignatureHint: "Onderaan de berichten die u verzendt…",
  settingsOrgFooter: "Organisatievoettekst",
  settingsOrgFooterHint:
    "Toegevoegd aan de uitgaande e-mail van elke gebruiker, na hun handtekening.",
  settingsOrgFooterPlaceholder:
    "bv. bedrijfsnaam, adres, wettelijke vermelding…",
  settingsOutOfOffice: "Afwezigheid",
  settingsOutOfOfficeHint:
    "Antwoord automatisch één keer aan iedereen die u mailt terwijl u afwezig bent.",
  settingsOooSubjectPlaceholder: "Onderwerp (optioneel) — bv. Afwezig",
  settingsOooMessagePlaceholder:
    "bv. Ik ben afwezig tot maandag en antwoord bij mijn terugkeer.",
  settingsOooNeedsMessage:
    "Voeg een bericht toe om afwezigheid in te schakelen.",
  settingsSave: "Opslaan",
  settingsSaved: "Opgeslagen.",
  settingsSaveError: "Kon uw instellingen niet opslaan.",
  settingsLoadError: "Kon uw instellingen niet laden.",

  // admin console
  adminTitle: "Beheer",
  adminBackToalo: "Terug naar alo",
  adminOpen: "Beheerconsole",
  adminOverview: "Overzicht",
  adminOverviewIntro: "Uw organisatie in één oogopslag.",
  overviewUsers: "Gebruikers",
  overviewStorage: "Gebruikte opslag",
  overviewDeliverability: "Bezorgbaarheid",
  overviewDeliverOk: "Alle controles geslaagd",
  overviewDeliverAttention: "Vereist aandacht",
  overviewAi: "AI",
  overviewOn: "Aan",
  overviewOff: "Uit",
  overviewManage: "Beheren",
  adminDomains: "Domeinen",
  adminDomainsIntro:
    "Domeinen waarvoor deze organisatie e-mail verzendt en ontvangt, en hun verificatie.",
  adminDomainsError: "Kon domeinen niet laden.",
  adminDomainsEmpty: "Nog geen domeinen. Voeg er een toe om het te verifiëren.",
  adminAddDomain: "Domein toevoegen",
  dkimPublish: "Publiceer dit DKIM-record zodat uw e-mail ondertekend wordt",
  dkimRotate: "DKIM roteren",
  dkimRotateConfirm: (domain: string) =>
    `De DKIM-sleutel voor ${domain} roteren? Publiceer het nieuwe record; behoud het oude tot de e-mail het niet meer gebruikt.`,
  dkimRotated: (domain: string) =>
    `Nieuwe DKIM-sleutel voor ${domain} — publiceer het bijgewerkte record.`,
  adminAudit: "Auditlogboek",
  adminAuditIntro: "Wie wat wijzigde, en wanneer. Nieuwste eerst.",
  adminAuditError: "Kon het auditlogboek niet laden.",
  adminAuditEmpty: "Nog geen beheeracties vastgelegd.",
  auditBy: (actor: string) => `door ${actor}`,
  auditUnknownActor: "systeem",
  auditUserCreate: "Gebruiker aangemaakt",
  auditUserDelete: "Gebruiker verwijderd",
  auditUserAdmin: "Beheerdersrechten gewijzigd",
  auditAliasAdd: "Alias toegevoegd",
  auditAliasRemove: "Alias verwijderd",
  auditGroupCreate: "Groep aangemaakt",
  auditGroupDelete: "Groep verwijderd",
  auditGroupAddress: "Lijstadres gewijzigd",
  auditDomainRegister: "Domein geregistreerd",
  auditDomainVerify: "Domein geverifieerd",
  auditDomainDelete: "Domein verwijderd",
  auditTenantCreate: "Organisatie aangemaakt",
  auditTenantStatus: "Organisatiestatus gewijzigd",
  auditTenantQuota: "Opslagquota gewijzigd",

  // control plane
  controlOpen: "Beheerplatform",
  controlTitle: "Beheerplatform",
  controlDeniedTitle: "Operatortoegang vereist",
  controlDeniedBody:
    "Het beheerplatform is voor platformoperators. Uw account is er geen — vraag een operator als u toegang nodig hebt.",
  controlTenants: "Organisaties",
  controlTenantsIntro: "Elke organisatie op deze installatie.",
  controlTenantsError: "Kon organisaties niet laden.",
  controlTenantsEmpty: "Nog geen organisaties. Maak de eerste aan.",
  controlDomains: "Domeinen",
  controlDomainsIntro:
    "Domeinen waarvoor elke organisatie e-mail mag verzenden en ontvangen, en hun verificatie.",
  controlDomainsError: "Kon domeinen niet laden.",
  controlDomainsEmpty: "Nog geen domeinen geregistreerd.",
  tenantAdd: "Nieuwe organisatie",
  tenantName: "Naam van organisatie",
  tenantNameHint: "Acme bv",
  tenantAdminEmail: "E-mail eerste beheerder",
  tenantAdminPassword: "Wachtwoord eerste beheerder",
  tenantAdminPasswordHint: "minstens 12 tekens",
  tenantCreate: "Organisatie aanmaken",
  tenantInvalid:
    "Een naam, een geldig beheerders-e-mailadres en een wachtwoord van 12+ tekens zijn vereist.",
  tenantCreateError: "Kon die organisatie niet aanmaken.",
  tenantActive: "Actief",
  tenantSuspended: "Opgeschort",
  tenantSuspend: "Opschorten",
  tenantResume: "Hervatten",
  tenantDelete: "Organisatie verwijderen",
  tenantDeleteConfirm: (name: string) =>
    `"${name}" en al haar gegevens definitief verwijderen? Dit kan niet ongedaan worden gemaakt.`,
  tenantUsage: (n: number, size: string) =>
    `${n === 1 ? "1 gebruiker" : `${n} gebruikers`} · ${size}`,
  tenantQuota: "Quota",
  tenantQuotaPrompt: "Opslagquota in GB (laat leeg voor onbeperkt):",
  tenantQuotaUnlimited: "onbeperkt",
  tenantQuotaOf: (size: string) => `van ${size}`,
  domainAdd: "Domein toevoegen",
  domainTenant: "Eigenaar-organisatie",
  domainName: "Domein",
  domainRegister: "Registreren",
  domainInvalid: "Kies een organisatie en voer een geldig domein in.",
  domainCreateError: "Kon dat domein niet registreren.",
  domainActionError: "Dat lukte niet. Probeer het opnieuw.",
  domainVerified: "Geverifieerd",
  domainUnverified: "Niet geverifieerd",
  domainVerify: "Verifiëren",
  domainDelete: "Domein verwijderen",
  domainOwnedBy: (tenant: string) => `Eigendom van ${tenant}`,
  domainDeleteConfirm: (domain: string) =>
    `${domain} van deze installatie verwijderen?`,
  domainVerifiedOk: (domain: string) => `${domain} is geverifieerd.`,
  domainVerifyPending: (domain: string) =>
    `Nog geen passend DNS-TXT-record gevonden voor ${domain} — publiceer het en probeer opnieuw.`,
  domainPublishTitle: "Publiceer dit DNS-record",
  domainPublishIntro: (domain: string) =>
    `Om eigendom van ${domain} te bewijzen, publiceert u dit TXT-record en klikt u daarna op Verifiëren bij het domein.`,
  domainRecordName: "Recordnaam",
  domainRecordType: "Type",
  domainRecordValue: "Waarde",
  domainPublishDone: "Klaar",

  adminDeniedTitle: "Beheerderstoegang vereist",
  adminDeniedBody:
    "U hebt geen beheerderstoegang tot deze werkomgeving. Vraag een beheerder om die toe te kennen als u die nodig hebt.",
  adminSecurity: "Beveiliging en vertrouwen",
  adminSecurityIntro:
    "Hoe uw maildomein er voor de buitenwereld uitziet. Deze controles bevragen elke keer de live DNS en het MTA-STS-beleid.",
  securityFor: (domain: string) => `Controles voor ${domain}`,
  securityRecheck: "Controles opnieuw uitvoeren",
  securityChecking: "Live controles uitvoeren…",
  securityError: "Kon de controles niet uitvoeren — probeer het opnieuw.",
  securityPass: "Geslaagd",
  securityWarn: "Aandacht",
  securityFail: "Actie nodig",
  adminGroups: "Groepen en lijsten",
  adminGroupsIntro:
    "Groepen voor gedeelde toegang, en distributielijsten die e-mail naar hun leden verspreiden.",
  adminNewGroup: "Nieuwe groep",
  adminGroupsError: "Kon groepen niet laden.",
  groupName: "Groepsnaam",
  groupRename: "Naam wijzigen",
  groupCreate: "Groep aanmaken",
  groupListBadge: "Lijst",
  groupMembers: "Leden",
  groupMemberCount: (n: number) => (n === 1 ? "1 lid" : `${n} leden`),
  groupNoMembers: "Nog geen leden.",
  groupListAddress: "Lijstadres",
  groupListAddressHint:
    "E-mail naar dit adres wordt aan elk lid bezorgd. Laat leeg voor een gewone toegangsgroep.",
  groupAddressSave: "Adres opslaan",
  groupAddressClear: "Lijst uitschakelen",
  groupAddMember: "Lid toevoegen",
  groupDelete: "Groep verwijderen",
  groupDeleteConfirm: (name: string) =>
    `De groep „${name}” verwijderen? Leden behouden hun postvakken.`,
  groupCreateError:
    "Kon die groep niet aanmaken — de naam is mogelijk al in gebruik.",
  groupAddressError:
    "Kon dat adres niet instellen — het is mogelijk al in gebruik.",
  groupActionError: "Dat lukte niet — probeer het opnieuw.",
  groupClose: "Sluiten",
  adminUsers: "Gebruikers en postvakken",
  adminUsersIntro: "Mensen in uw organisatie en hun postvakken.",
  adminAddUser: "Gebruiker toevoegen",
  adminUsersError: "Kon gebruikers niet laden.",
  userAdminBadge: "Beheerder",
  userManage: "Beheren",
  userUsage: (n: number, size: string) =>
    `${n === 1 ? "1 bericht" : `${n} berichten`} · ${size}`,
  userEmail: "E-mail",
  userPassword: "Wachtwoord",
  userNewPassword: "Nieuw wachtwoord",
  userPasswordHint: "Minstens 8 tekens.",
  userCreate: "Gebruiker aanmaken",
  userInvalid:
    "Voer een geldig e-mailadres en een wachtwoord van minstens 8 tekens in.",
  userCreateError:
    "Kon die gebruiker niet aanmaken — het e-mailadres is mogelijk al in gebruik.",
  userReset: "Wachtwoord herstellen",
  userResetDone: "Wachtwoord hersteld.",
  userAdminRole: "Organisatiebeheerder",
  userAdminRoleFor: (email: string) => `Organisatiebeheer voor ${email}`,
  userAdminHint:
    "Beheerders kunnen gebruikers, aliassen en instellingen beheren.",
  userAliases: "Aliassen",
  userAliasesHint: "Extra adressen die naar dit postvak worden bezorgd.",
  userAliasPlaceholder: "alias@namel3ss.com",
  userAliasAdd: "Alias toevoegen",
  userDelete: "Gebruiker verwijderen",
  userDeleteConfirm: (email: string) =>
    `${email} en al hun e-mail verwijderen? Dit kan niet ongedaan worden gemaakt.`,
  userActionError: "Dat lukte niet — probeer het opnieuw.",
  userClose: "Sluiten",
  adminAiProviders: "AI-providers",
  adminProviderEnabledFor: (name: string) => `${name} ingeschakeld`,
  adminAiIntro:
    "Kies welke modellen alo aandrijven — zelf gehost, of uw eigen API-sleutels.",
  adminAddProvider: "Provider toevoegen",
  adminManage: "Beheren",
  adminDefaultBadge: "Standaard",
  adminMakeDefault: "Als standaard instellen",
  adminProvidersError: "Kon providers niet laden.",
  adminAiSelfHosted: "Zelf gehost (aanbevolen)",
  adminAiSelfHostedHint:
    "Draait op uw eigen infrastructuur — geen data verlaat uw servers.",
  adminAiOwnKeys: "Uw eigen API-sleutels",
  adminAiOwnKeysHint:
    "Verbind een externe provider met uw sleutel. Verzoeken verlaten uw server naar die provider.",
  adminAiFootnote:
    "Zelf gehoste providers houden alle data op uw infrastructuur. Externe API-sleutels sturen verzoeken en inhoud naar die provider — kies volgens uw databeleid.",
  providerConnected: "Verbonden",
  providerKeyAdded: "Sleutel toegevoegd",
  providerReady: "Klaar",
  providerNotConfigured: "Niet geconfigureerd",
  kindOllama: "Ollama",
  kindalo: "alo AI",
  kindMistral: "Mistral (EU)",
  mistralDesc:
    "Europese modellen, gehost in de EU. Voeg je Mistral-sleutel toe om in te schakelen. Aanbevolen voor datasoevereiniteit.",
  kindOpenai: "OpenAI",
  kindAnthropic: "Anthropic",
  kindCustom: "Aangepast eindpunt",
  builtInTag: "Ingebouwd",
  ollamaDesc:
    "Lokale modellen op uw server — Llama 3, Mistral en meer. Volledig privé.",
  aloDesc:
    "Ingebouwd, EU-gehost model afgestemd op alo — richt het op uw alo AI-eindpunt.",
  openaiDesc:
    "GPT-4o, GPT-4o mini. Voeg uw OpenAI-sleutel toe om in te schakelen.",
  anthropicDesc:
    "Claude-modellen. Voeg uw Anthropic API-sleutel toe om in te schakelen.",
  customDesc:
    "Elke OpenAI-compatibele API — zelf gehoste vLLM, Together, Groq, OpenRouter…",
  connectTitle: (name: string) => `${name} verbinden`,
  configureTitle: (name: string) => `${name} configureren`,
  providerBaseUrl: "API-eindpunt",
  providerModel: "Model",
  providerModels: "Ingeschakelde modellen",
  providerAddModel: "Toevoegen",
  providerModelPlaceholder: "modelnaam",
  providerRemoveModel: (name: string) => `${name} verwijderen`,
  providerApiKey: "API-sleutel",
  providerShowKey: "Sleutel tonen",
  providerHideKey: "Sleutel verbergen",
  providerApiKeyKept:
    "Opgeslagen — laat leeg om de huidige sleutel te behouden",
  providerApiKeyOptional: "Niet nodig voor een lokale Ollama",
  providerTest: "Verbinding testen",
  providerTestAgain: "Opnieuw testen",
  providerTesting: "Testen…",
  providerTestOk: (n: number) =>
    n === 1
      ? "Verbinding geverifieerd — 1 model bereikbaar"
      : `Verbinding geverifieerd — ${n} modellen bereikbaar`,
  providerTestFail: "Kon dat eindpunt niet bereiken.",
  providerCancel: "Annuleren",
  providerSave: "Opslaan en inschakelen",
  providerSaveError: "Kon die provider niet opslaan.",
  providerRequired: "Een eindpunt en een model zijn vereist.",
  removeRecipient: (name: string) => `${name} verwijderen`,
  recipientCount: (n: number) => (n === 1 ? "1 ontvanger" : `${n} ontvangers`),

  aiComingSoon: "De AI-assistent komt binnenkort.",
  archiveUnavailable: "Er is geen archiefmap om dit naartoe te verplaatsen.",

  // Docs
  docTitle: "Q3 Offerte — Proceq",
  docSaved: "Opgeslagen in Drive · alle wijzigingen opgeslagen",
  docViewMode: "Documentweergave",
  docCanvasView: "Canvas",
  docCanvasViewHint: "Flexibele canvasweergave",
  docPageView: "Pagina",
  docPageViewHint: "Afdrukweergave als pagina",
  docFormattingToolbar: "Werkbalk voor documentopmaak",
  docMenuFile: "Bestand",
  docMenuEdit: "Bewerken",
  docMenuInsert: "Invoegen",
  docMenuFormat: "Opmaak",
  docPrint: "Afdrukken",
  docInsertDivider: "Scheidingslijn",
  docInsertPageBreak: "Pagina-einde",
  docZoom: "Documentzoom",
  docZoomOut: "Uitzoomen",
  docZoomIn: "Inzoomen",
  docParagraphStyle: "Alineastijl",
  docStyleParagraph: "Alinea",
  docStyleHeading1: "Kop 1",
  docStyleHeading2: "Kop 2",
  docStyleHeading3: "Kop 3",
  docStyleBulletList: "Opsommingstekens",
  docStyleNumberedList: "Genummerde lijst",
  docStyleChecklist: "Controlelijst",
  docTextColor: "Tekstkleur",
  docHighlightColor: "Markeringskleur",
  docHighlightNone: "Geen markering",
  docColorDefault: "Standaardkleur",
  docColorHex: "Hex",
  docColorOpacity: "Dekking",
  docColorEyedropper: "Kies een kleur van het scherm",
  docBrandColors: "Merkkleuren",
  docSaveBrandColor: "Huidige merkkleur opslaan",
  docRemoveBrandColor: "Merkkleur verwijderen",
  docColorRed: "Rood",
  docColorOrange: "Oranje",
  docColorYellow: "Geel",
  docColorGreen: "Groen",
  docColorBlue: "Blauw",
  docColorPurple: "Paars",
  docIndent: "Inspringing vergroten",
  docOutdent: "Inspringing verkleinen",
  docWords: "woorden",
  docCharacters: "tekens",
  docInsertLink: "Link invoegen",
  docLinkPrompt: "Voer het webadres voor de geselecteerde tekst in",
  docInsertImage: "Afbeelding invoegen",
  docFindReplace: "Zoeken en vervangen",
  docFind: "Zoeken",
  docReplaceWith: "Vervangen door",
  docFindNext: "Volgende zoeken",
  docReplaceAll: "Alles vervangen",
  docPageSetup: "Pagina-instelling",
  docPageSize: "Paginaformaat",
  docPageLetter: "Letter",
  docPageOrientation: "Afdrukstand",
  docPagePortrait: "Staand",
  docPageLandscape: "Liggend",
  docPageMargins: "Marges",
  docMarginsNormal: "Normaal",
  docMarginsNarrow: "Smal",
  docMarginsWide: "Breed",
  docHeader: "Koptekst",
  docHeaderPlaceholder: "Koptekst",
  docFooter: "Voettekst",
  docFooterPlaceholder: "Voettekst",
  docPageNumbers: "Paginanummer tonen",
  docFontFamily: "Lettertype",
  docFontSize: "Lettergrootte",
  docLineSpacing: "Regelafstand",
  docAddComment: "Opmerking toevoegen",
  docComment: "Opmerking",
  docCommentPlaceholder: "Schrijf een opmerking…",
  docResolveComment: "Opmerking oplossen",
  docReopenComment: "Opmerking heropenen",
  docSavePdf: "Opslaan als PDF",
  docAiPlaceholder: "Vertel de AI wat te schrijven of wijzigen…",
  docAiPropose: "Opstellen",
  docAiProposalLabel: "Voorstel — controleer voor je het toevoegt",
  docAiInsert: "Invoegen",
  docAiDiscard: "Verwerpen",
  docAiUnavailable: "AI is momenteel niet beschikbaar.",
  docAskAi: "Vraag AI",
  docEquation: "Vergelijking",
  docEquationHint: "Wiskundige formule (LaTeX)",
  docBlockGroupAdvanced: "Geavanceerd",
  driveImporting: (name: string): string => `${name} importeren…`,
  driveImportNote:
    "We openen dit als een alo Sheet. Sommige opmaak kan afwijken — je oorspronkelijke bestand blijft ongewijzigd in Drive.",
  driveImportFailed: (name: string): string =>
    `Kon ${name} niet importeren. Je kunt het origineel nog steeds downloaden.`,
  sheetDownloadXlsx: "Downloaden als Excel (.xlsx)",
  sheetDownloadXlsxShort: "Excel",
  sheetName: "Bladnaam",
  sheetSaved: "Opgeslagen",
  sheetExport: "Exporteren",
  sheetMore: "Meer acties",
  sheetRibbon: "Opmaak",
  sheetTabHome: "Start",
  sheetTabOthers: "Overig",
  sheetTabInsert: "Invoegen",
  sheetTabDraw: "Tekenen",
  sheetTabLayout: "Pagina-indeling",
  sheetTabFormulas: "Formules",
  sheetTabData: "Gegevens",
  sheetTabReview: "Controleren",
  sheetTabView: "Beeld",
  sheetTabSoon: (name: string): string =>
    `${name}-hulpmiddelen komen binnenkort.`,
  sheetGroupCellSize: "Celgrootte",
  sheetRowHeight: "Rijhoogte",
  sheetColumnWidth: "Kolombreedte",
  sheetAutoFitRow: "Rij automatisch aanpassen",
  sheetAutoFitColumn: "Kolom automatisch aanpassen",
  sheetGroupVisibility: "Zichtbaarheid",
  sheetHideRow: "Geselecteerde rij verbergen",
  sheetShowRows: "Alle rijen weergeven",
  sheetHideColumn: "Geselecteerde kolom verbergen",
  sheetShowColumns: "Alle kolommen weergeven",
  sheetGroupSheetOptions: "Bladopties",
  sheetToggleGridlines: "Rasterlijnen",
  sheetGridlineColor: "Kleur rasterlijnen",
  sheetGroupDirection: "Richting",
  sheetLeftToRight: "Links naar rechts",
  sheetRightToLeft: "Rechts naar links",
  sheetUndo: "Ongedaan maken",
  sheetRedo: "Opnieuw",
  sheetGroupHistory: "Ongedaan maken",
  sheetGroupFont: "Lettertype",
  sheetGroupBorders: "Randen",
  sheetGroupRotation: "Rotatie",
  sheetGroupAlignment: "Uitlijning",
  sheetGroupWrap: "Tekstomloop",
  sheetGroupMerge: "Samenvoegen",
  sheetWrapOverflow: "Overlopen",
  sheetWrapText: "Tekstterugloop",
  sheetWrapClip: "Afkappen",
  sheetMergeAll: "Alles samenvoegen",
  sheetMergeAcross: "Horizontaal samenvoegen",
  sheetMergeVertically: "Verticaal samenvoegen",
  sheetUnmerge: "Samenvoeging opheffen",
  sheetGroupNumber: "Getal",
  sheetFontFamily: "Lettertype",
  sheetFontSize: "Tekengrootte",
  sheetBold: "Vet",
  sheetItalic: "Cursief",
  sheetUnderline: "Onderstrepen",
  sheetStrike: "Doorhalen",
  sheetAlignLeft: "Links uitlijnen",
  sheetAlignCenter: "Centreren",
  sheetAlignRight: "Rechts uitlijnen",
  sheetMerge: "Cellen samenvoegen",
  sheetNumberFormat: "Getalnotatie",
  sheetCellStyles: "Celstijlen",
  sheetMoreStyles: "Meer celstijlen",
  sheetStyleDefault: "Standaard",
  sheetStyleHeading1: "Kop 1",
  sheetStyleHeading2: "Kop 2",
  sheetStyleHeading3: "Kop 3",
  sheetStyleHeading4: "Kop 4",
  sheetStyleTitle: "Titel",
  sheetStyleSubtitle: "Subtitel",
  sheetFormatGeneral: "Algemeen",
  sheetFormatNumber: "Getal",
  sheetFormatCurrency: "Valuta",
  sheetFormatPercentage: "Percentage",
  sheetFormatDate: "Datum",
  sheetFormatText: "Tekst",
  sheetFormatPreviewGeneral: "1234,56",
  sheetFormatPreviewNumber: "1.234,56",
  sheetFormatPreviewCurrency: "€ 1.234,56",
  sheetFormatPreviewPercentage: "12,34%",
  sheetFormatPreviewDate: "06-08-2026",
  sheetFormatPreviewText: "Tekst",
  sheetFontGrow: "Tekst vergroten",
  sheetFontShrink: "Tekst verkleinen",
  sheetFontColor: "Tekstkleur",
  sheetFillColor: "Opvulkleur",
  sheetAlignTop: "Boven uitlijnen",
  sheetAlignMiddle: "Midden uitlijnen",
  sheetAlignBottom: "Onder uitlijnen",
  sheetWrap: "Tekstterugloop",
  sheetGroupCells: "Cellen",
  sheetInsert: "Invoegen",
  sheetDelete: "Verwijderen",
  sheetFormat: "Opmaak",
  sheetMoreCellOptions: "Meer celopties",
  sheetSortFilter: "Sorteren en filteren",
  sheetGroupClear: "Wissen",
  sheetGroupRows: "Rijen",
  sheetGroupColumns: "Kolommen",
  sheetGroupView: "Venster",
  sheetInsertRowAbove: "Rij hierboven invoegen",
  sheetInsertRowBelow: "Rij hieronder invoegen",
  sheetInsertColLeft: "Kolom links invoegen",
  sheetInsertColRight: "Kolom rechts invoegen",
  sheetDeleteRow: "Rij verwijderen",
  sheetDeleteColumn: "Kolom verwijderen",
  sheetClearContents: "Inhoud wissen",
  sheetClearFormats: "Opmaak wissen",
  sheetFreeze: "Titels blokkeren",
  sheetUnfreeze: "Deblokkeren",
  sheetGroupClipboard: "Klembord",
  sheetGroupStyles: "Stijlen",
  sheetGroupEditing: "Bewerken",
  sheetGroupSortFilter: "Sorteren en filteren",
  sheetGroupDataTools: "Gegevenshulpmiddelen",
  sheetGroupProtection: "Beveiliging",
  sheetGroupFreeze: "Vensters vastzetten",
  sheetGroupZoom: "Zoomen",
  sheetGroupInsertObjects: "Objecten",
  sheetGroupDrawing: "Tekenen",
  sheetGroupNotes: "Notities",
  sheetGroupComments: "Opmerkingen",
  sheetGroupFunctionLibrary: "Functiebibliotheek",
  sheetGroupMoreFunctions: "Meer functies",
  sheetAutoSum: "AutoSom",
  sheetAverage: "Gemiddelde",
  sheetCount: "Aantal",
  sheetMinimum: "Minimum",
  sheetMaximum: "Maximum",
  sheetMoreFunctions: "Functies bekijken",
  sheetGroupFunctionCategories: "Functiecategorieën",
  sheetFormulaFinancial: "Financieel",
  sheetFormulaDateTime: "Datum en tijd",
  sheetFormulaMathTrig: "Wiskunde en trigonometrie",
  sheetFormulaStatistical: "Statistisch",
  sheetFormulaLookup: "Zoeken en verwijzen",
  sheetFormulaDatabase: "Database",
  sheetFormulaText: "Tekst",
  sheetFormulaLogical: "Logisch",
  sheetFormulaInformation: "Informatie",
  sheetFormulaEngineering: "Techniek",
  sheetFormulaCube: "Kubus",
  sheetFormulaCompatibility: "Compatibiliteit",
  sheetFormulaWeb: "Web",
  sheetFormulaArray: "Matrix",
  sheetDataValidation: "Gegevensvalidatie",
  sheetConditionalFormatting: "Voorwaardelijke opmaak",
  sheetTextToColumns: "Tekst naar kolommen",
  sheetNamedRanges: "Benoemde bereiken",
  sheetProtectRange: "Bereik beveiligen",
  sheetUnprotectRange: "Bereikbeveiliging opheffen",
  sheetProtectSheet: "Blad beveiligen",
  sheetUnprotectSheet: "Bladbeveiliging opheffen",
  sheetProtectedRangeName: "Beveiligd bereik",
  sheetProtectedSheetName: "Beveiligd blad",
  sheetFreezeTopRow: "Bovenste rij vastzetten",
  sheetFreezeFirstColumn: "Eerste kolom vastzetten",
  sheetZoomOut: "Uitzoomen",
  sheetZoomReset: "100%",
  sheetZoomIn: "Inzoomen",
  sheetInsertTable: "Tabel",
  sheetInsertLink: "Koppeling",
  sheetInsertImage: "Afbeelding",
  sheetDrawingPanel: "Afbeeldingen en tekenen",
  sheetNote: "Notitie toevoegen of bewerken",
  sheetAddComment: "Nieuwe opmerking",
  sheetCommentsPanel: "Opmerkingenvenster",
  sheetPaste: "Plakken",
  sheetCut: "Knippen",
  sheetCopy: "Kopiëren",
  sheetPercent: "Percentage",
  sheetCurrency: "Valuta",
  sheetComma: "Duizendtalscheidingsteken",
  sheetSortAsc: "Sorteren A → Z",
  sheetSortDesc: "Sorteren Z → A",
  sheetFilter: "Filter aan/uit",
  sheetFindReplace: "Zoeken en vervangen",
  sheetBorders: "Randen",
  sheetBordersAll: "Alle randen",
  sheetBordersOuter: "Buitenrand",
  sheetBordersInside: "Binnenranden",
  sheetBordersTop: "Bovenrand",
  sheetBordersBottom: "Onderrand",
  sheetBordersLeft: "Linkerrand",
  sheetBordersRight: "Rechterrand",
  sheetBordersHorizontal: "Horizontale randen",
  sheetBordersVertical: "Verticale randen",
  sheetBordersNone: "Geen rand",
  sheetBordersAdvanced: "Diagonale randen",
  sheetBordersDiagonalDown: "Diagonale rand omlaag",
  sheetBordersDiagonalUp: "Diagonale rand omhoog",
  sheetBordersDiagonalDownCenter: "Diagonaal omlaag met middenlijnen",
  sheetBordersDiagonalDownBoth: "Diagonaal omlaag met beide middenlijnen",
  sheetBordersDiagonalUpCenter: "Diagonaal omhoog met middenlijnen",
  sheetRotation: "Rotatie",
  sheetRotationNone: "Geen rotatie",
  sheetRotation45: "45° rechtsom draaien",
  sheetRotationMinus45: "45° linksom draaien",
  sheetRotation90: "90° rechtsom draaien",
  sheetRotationMinus90: "90° linksom draaien",
  sheetRotationVertical: "Verticale tekst",
  docShare: "Delen",
  docInsert: "Invoegen",
  insertEquation: "Vergelijking",
  insertCrossRef: "Kruisverwijzing",
  tbNormalText: "Normale tekst",
  tbEditing: "Bewerken",
  eqTitle: "Vergelijking",
  eqClose: "Sluiten",
  eqInsert: "Invoegen",
  eqPlaceholder: "bv.  E = mc^2",
  eqInputLabel: "LaTeX-bron",
  eqPreview: "Voorbeeld",
  eqEmpty: "Begin hierboven LaTeX te typen.",
  eqError: (message: string) => `Kan deze LaTeX niet weergeven: ${message}`,
  eqNumbered: "Genummerd",
  eqEmptyBlock: "Lege vergelijking — klik om te bewerken",
  eqSearchLabel: "Symbolen zoeken",
  eqSearchPlaceholder: "Symbolen zoeken — bv. som, alfa, pijl",
  eqSearchClear: "Zoekopdracht wissen",
  eqNoMatches: "Geen symbolen gevonden voor uw zoekopdracht.",
  eqCatStructures: "Structuren",
  eqCatStyles: "Lettertypes en stijlen",
  eqCatGreek: "Grieks",
  eqCatOperators: "Operatoren",
  eqCatRelations: "Relaties",
  eqCatSets: "Verzamelingen en logica",
  eqCatArrows: "Pijlen",
  eqCatBigops: "Grote operatoren",
  eqCatCalculus: "Analyse",
  eqCatDelimiters: "Scheidingstekens",
  eqCatMisc: "Symbolen",
  composeInsertEquation: "Vergelijking invoegen",
  composeInsertCode: "Codeblok invoegen",
  strikethrough: "Doorhalen",
  textColor: "Tekstkleur",
  highlight: "Markeren",
  bulletList: "Opsommingslijst",
  numberedList: "Genummerde lijst",
  alignLeft: "Links uitlijnen",
  alignCenter: "Centreren",
  alignRight: "Rechts uitlijnen",
  horizontalRule: "Scheidingslijn",
  insertImage: "Afbeelding invoegen",
  clearFormatting: "Opmaak wissen",
  textStyle: "Tekststijl",
  styleQuote: "Citaat",
  fontFamily: "Lettertype",
  fontSize: "Tekengrootte",
  sizeSmall: "Klein",
  sizeNormal: "Normaal",
  sizeLarge: "Groot",
  sizeHuge: "Zeer groot",
  codeInsertTitle: "Codeblok invoegen",
  codeInsertHint: "⌘/Ctrl + Enter om in te voegen",
  codePreviewLabel: "Voorbeeld — hoe het er in de e-mail uitziet",
  insertCancel: "Annuleren",
  insertConfirm: "Invoegen",
  docsTitle: "alo Documenten",
  docsNew: "Nieuw document",
  docsEmpty: "Nog geen documenten. Maak er een om te beginnen schrijven.",
  docsDelete: (title: string) => `${title} verwijderen`,
  docsAll: "Alle documenten",
  docsUntitled: "Naamloos document",
  docsTitleLabel: "Documenttitel",
  docsSaving: "Opslaan…",
  docsSaved: "Opgeslagen",
  docsSaveError: "Kon niet opslaan",
  blockAdd: "Een blok toevoegen",
  blockMoveUp: "Blok omhoog verplaatsen",
  blockMoveDown: "Blok omlaag verplaatsen",
  blockDelete: "Blok verwijderen",
  blockEmptyHint:
    "Voeg een kop, tekst, vergelijking, code of tabel toe om te beginnen.",
  headingH1: "Kop 1",
  headingH2: "Kop 2",
  headingPlaceholder: "Sectiekop",
  headingLabel: "Koptekst",
  paraPlaceholder:
    "Schrijf hier. Gebruik de werkbalk om inline-wiskunde of een kruisverwijzing in te voegen.",
  paraLabel: "Alineatekst",
  paraInlineMath: "Inline-wiskunde",
  paraReference: "Verwijzing",
  paraToolbar: "In deze alinea invoegen",
  tableHeaderCell: "Kolomkop",
  tableCell: "Cel",
  tableAddRow: "Rij toevoegen",
  tableAddColumn: "Kolom toevoegen",
  tableRemoveRow: "Rij verwijderen",
  tableRemoveColumn: "Kolom verwijderen",
  tableBlockLabel: "Bewerkbare tabel",
  codeSearchLanguage: "Taal zoeken…",
  codeNoLanguage: "Geen passende taal",
  codeCopy: "Kopiëren",
  codeCopied: "Gekopieerd",
  codeInputLabel: "Code",
  codePlaceholder: "Plak of typ uw code…",
  codeWrap: "Regelterugloop",
  refSection: "Sectie",
  refEquation: "Vgl.",
  refTable: "Tabel",
  refFigure: "Figuur",
  refBroken: "verbroken verwijzing",
  refInsert: "Kruisverwijzing invoegen",
  refInsertTitle: "Kruisverwijzing invoegen",
  refClose: "Sluiten",
  refNoneOfKind: "Nog niets van dit type.",
  refTabEquations: "Vergelijkingen",
  refTabSections: "Secties",
  refTabTables: "Tabellen",
  refTabFigures: "Figuren",
  driveLoadingFile: (name: string) => `${name} openen…`,
  driveOpeningEditor: "je bestand",
  driveFileOpenFailedTitle: "Dit bestand is niet geopend",
  driveFileUnavailable:
    "Het bestand is mogelijk verplaatst of verwijderd. Ga terug naar je bestanden en kies een ander item.",
  driveEditorLoadFailed: (reason: string) =>
    `Drive kon dit bestand niet openen. ${reason}`,
  driveBackToFiles: "Terug naar bestanden",

  // Facturatiegereedschap van de agent (ADR 0035, B1.25). Elk levert een
  // concept op: goedkeuren geeft niets uit, nummert niets en verstuurt niets.
  agentActInvoiceDraft: "Conceptfactuur",
  agentActQuoteToInvoice: "Offerte accepteren",
  agentActPaymentReminder: "Betalingsherinnering",
  agentFieldCustomer: "Klant",
  agentFieldLines: "Regels",
  agentFieldQuote: "Offerte",
  agentFieldInvoice: "Factuur",
  agentLineCount: (n: number): string => (n === 1 ? "1 regel" : `${n} regels`),
  agentInvoiceDraftNote:
    "Maakt een concept — er wordt niets uitgegeven, genummerd of verstuurd.",
  agentQuoteToInvoiceNote:
    "Sluit de offerte als geaccepteerd en maakt een conceptfactuur.",
  agentReminderNote:
    "Schrijft een herinnering in uw Concepten — er wordt niets verstuurd.",

  // alo Facturatie (ADR 0035, golf B1) — klanten en de prijslijst. De module
  // spreekt over documenten ("een factuur opmaken"), niet over rijen, en
  // noemt nooit een validatieregel die van de server is: een weigering staat
  // er in de woorden van de server, zodat de twee elkaar nooit kunnen
  // tegenspreken.
  moduleBilling: "Facturatie",
  billingCustomers: "Klanten",
  billingProducts: "Prijslijst",
  billingSearchCustomers: "Klanten zoeken…",
  billingSearchProducts: "In de prijslijst zoeken…",
  billingShowArchived: "Gearchiveerde tonen",
  billingArchived: "Gearchiveerd",
  billingArchive: "Archiveren",
  billingRestore: "Herstellen",
  billingNewCustomer: "Nieuwe klant",
  billingNewProduct: "Nieuw artikel",
  billingEditCustomer: "Klant bewerken",
  billingEditProduct: "Artikel bewerken",
  billingCustomerSubtitle: "Aan wie uw facturen zijn gericht.",
  billingProductSubtitle:
    "Een artikel dat u kunt kiezen wanneer u een document opmaakt.",
  billingArchiveCustomerConfirm: (name: string) =>
    `${name} archiveren? Ze verdwijnen uit de keuzelijsten; elk document dat al is opgemaakt blijft ze noemen.`,
  billingArchiveProductConfirm: (name: string) =>
    `${name} archiveren? Het verdwijnt uit de keuzelijsten; documenten die al zijn opgemaakt houden de prijs waarmee ze zijn opgemaakt.`,
  billingCreate: "Aanmaken",
  billingSave: "Opslaan",
  billingCancel: "Annuleren",
  billingLoadFailed:
    "Deze lijst kon niet worden geladen. Controleer uw verbinding en probeer opnieuw.",
  billingLoading: "Factuurgegevens laden…",
  billingSaveFailed:
    "Opslaan is niet gelukt. Controleer uw verbinding en probeer opnieuw.",
  billingNoMatches: "Niets komt overeen met die zoekopdracht.",
  billingNoCustomersTitle: "Nog geen klanten",
  billingGetStarted: "Begin in 3 eenvoudige stappen",
  billingStepCustomerTitle: "Voeg uw eerste klant toe",
  billingStepCustomerBody: "Maak een klantprofiel met de factuurgegevens.",
  billingStepInvoiceTitle: "Maak uw eerste factuur",
  billingStepInvoiceBody: "Voeg artikelen toe, stel betaalvoorwaarden in en verstuur de factuur.",
  billingStepPaidTitle: "Word sneller betaald",
  billingStepPaidBody: "Registreer betalingen en houd uw kasstroom bij.",
  billingNoCustomersBody:
    "Een klant draagt het adres, het btw-nummer en de betaaltermijn waarmee elke factuur die u voor hen opmaakt begint.",
  billingNoProductsTitle: "Uw prijslijst is leeg",
  billingNoProductsBody:
    "Leg één keer vast wat u verkoopt en kies het daarna wanneer u een offerte of factuur opmaakt.",
  billingColName: "Naam",
  billingColLocation: "Plaats",
  billingColVatId: "Btw-nummer",
  billingColEmail: "E-mail",
  billingColTerms: "Betaaltermijn",
  billingColCurrency: "Valuta",
  billingColUnit: "Eenheid",
  billingColUnitPrice: "Stukprijs",
  billingColVatRate: "Btw-tarief",
  billingColActions: "Acties",
  billingTermsDays: (days: number) => (days === 1 ? "1 dag" : `${days} dagen`),
  billingFieldName: "Naam",
  billingFieldEmail: "Factuur-e-mail",
  billingFieldAddress: "Adres",
  billingFieldAddress2: "Adres, tweede regel",
  billingFieldPostalCode: "Postcode",
  billingFieldCity: "Plaats",
  billingFieldCountry: "Land",
  billingFieldVatId: "Btw-nummer",
  billingFieldTerms: "Betaaltermijn (dagen)",
  billingFieldCurrency: "Valuta",
  billingFieldUnit: "Eenheid",
  billingFieldUnitPrice: "Stukprijs",
  billingFieldVatRate: "Btw-tarief (%)",
  billingEmailPlaceholder: "facturatie@voorbeeld.nl",
  billingAddressPlaceholder: "Straat en huisnummer",
  billingCountryPlaceholder: "NL",
  billingCountryHint: "Landcode van twee letters.",
  billingCurrencyPlaceholder: "EUR",
  billingVatIdPlaceholder: "NL123456789B01",
  billingVatIdHint: "Laat leeg voor een particuliere klant.",
  billingTermsPlaceholder: "30",
  billingTermsHint: "Dagen tussen uitgifte en vervaldatum.",
  billingUnitPlaceholder: "uur",
  billingUnitHint: "Hoe één ervan heet. Laat leeg voor een vast bedrag.",
  billingAmountPlaceholder: "0,00",
  billingPriceHint: "Exclusief btw.",
  billingRatePlaceholder: "21",
  billingRateHint: "0 voor een vrijgesteld artikel.",
  billingNotAnAmount: "Voer een bedrag in zoals 1250,00.",
  billingNotARate: "Voer een tarief in zoals 21.",

  // Facturen (B1.14): de lijst en de conceptbewerker. Elk bedrag dat u hier
  // leest is dat van de server — de tekst belooft nooit een totaal dat de
  // browser heeft uitgerekend, en zegt het gewoon wanneer een bedrag één
  // wijziging achterloopt.
  billingInvoices: "Facturen",
  billingNewInvoice: "Nieuwe factuur",
  billingSearchInvoices: "Zoeken op nummer, klant of referentie…",
  billingFilterStatus: "Tonen",
  billingFilterAll: "Alle documenten",
  billingStatusDraft: "Concept",
  billingStatusIssued: "Uitgegeven",
  billingStatusPaid: "Betaald",
  billingStatusVoid: "Geannuleerd",
  billingStatusOverdue: "Achterstallig",
  billingCreditNote: "Creditnota",
  billingCreditNotes: "Creditnota’s",
  billingNoInvoicesTitle: "Nog geen facturen",
  billingNoInvoicesBody:
    "Maak een concept op voor een klant, zet erbij wat u in rekening brengt, en geef het uit wanneer het klopt.",
  billingColNumber: "Nummer",
  billingColCustomer: "Klant",
  billingColIssueDate: "Uitgiftedatum",
  billingColDueDate: "Vervaldatum",
  billingColStatus: "Status",
  billingColTotal: "Totaal",
  billingColDescription: "Omschrijving",
  billingColQty: "Aantal",
  billingColNet: "Netto",
  billingNotNumbered: "—",
  billingNoDate: "—",
  billingUnknownCustomer: "Onbekende klant",
  billingDraftInvoice: "Conceptfactuur",
  billingBackToInvoices: "Alle facturen",
  billingInvoiceGone: "Dit document bestaat niet meer.",
  billingFieldCustomer: "Klant",
  billingChooseCustomer: "Kies een klant…",
  billingCustomerFixedHint:
    "Hun valuta en betaaltermijn worden op het document overgenomen.",
  billingFieldReference: "Hun referentie",
  billingReferencePlaceholder: "PO-1234",
  billingReferenceHint:
    "Het ordernummer van de klant zelf, gedrukt op het document.",
  billingFieldNote: "Notitie",
  billingNotePlaceholder: "Wat de klant op het document moet lezen.",
  billingNoteHint: "Gedrukt onder de regels.",
  billingFieldIssueDate: "Uitgiftedatum",
  billingFieldDueDate: "Vervaldatum",
  billingCreateDraft: "Concept aanmaken",
  billingCreateDraftHint:
    "Eerst wordt het concept opgemaakt; daarna zet u erbij wat u in rekening brengt.",
  billingLines: "Regels",
  billingAddLine: "Regel toevoegen",
  billingRemoveLine: "Deze regel verwijderen",
  billingNoLines: "Nog niets op dit document.",
  billingPickProduct: "Uit de prijslijst…",
  billingDescriptionPlaceholder: "Wat u in rekening brengt",
  billingQtyPlaceholder: "1",
  billingLineNeedsDescription:
    "Een regel heeft een omschrijving nodig voordat het concept kan worden opgeslagen.",
  billingNotAQuantity: "Voer een aantal in zoals 1,5.",
  billingTotalsNet: "Netto",
  billingTotalsGross: "Totaal",
  billingVatAtRate: (rate: string) => `Btw ${rate}`,
  billingTotalsStale:
    "Dit zijn de laatste bedragen die de server stuurde; ze worden bijgewerkt zodra het concept is opgeslagen.",
  billingSaving: "Opslaan…",
  billingSaved: "Opgeslagen",
  billingUnsaved: "Nog niet opgeslagen",
  billingSaveNotDone: "Opslaan niet gelukt",
  billingSaveNow: "Opnieuw proberen",
  billingDeleteDraft: "Concept verwijderen",
  billingDeleteDraftConfirm:
    "Dit concept verwijderen? Het draagt geen nummer, dus er blijft niets achter — en er kan niets worden teruggehaald.",
  billingFrozenNotice:
    "Dit document draagt een nummer en kan niet meer worden gewijzigd. Corrigeer het met een creditnota.",

  // Levensloop (B1.15). Elk van deze acties is onomkeerbaar op een juridisch
  // document, dus de bevestiging zegt wat ze DOET — een nummer verbruiken, de
  // prijzen vastzetten, de offerte sluiten — in plaats van te vragen of u het
  // zeker weet. Geen ervan belooft een e-mail.
  billingActionFailed:
    "Dat is niet gelukt. Controleer uw verbinding en probeer opnieuw.",
  billingActionsWaitForSave:
    "Deze wachten tot uw laatste wijziging is opgeslagen.",
  billingIssue: "Uitgeven",
  billingIssueTitle: "Deze factuur uitgeven?",
  billingIssueConfirm:
    "Uitgeven neemt het volgende nummer uit uw reeks, dateert het document en zet het vast. Het kan daarna nooit meer worden gewijzigd — een fout wordt achteraf met een creditnota gecorrigeerd. Er wordt niets naar de klant gemaild.",
  billingVoid: "Annuleren",
  billingVoidTitle: "Deze factuur annuleren?",
  billingVoidConfirm:
    "Een geannuleerde factuur houdt haar nummer en blijft leesbaar, maar is niets meer waard. Annuleer er een die niemand heeft gezien; heeft de klant dit document al, maak dan een creditnota.",
  billingVoidNotice:
    "Deze factuur is geannuleerd. Ze houdt haar nummer en is niets waard.",
  billingCreditNoteAction: "Creditnota",
  billingCreditNoteTitle: "Een creditnota opmaken?",
  billingCreditNoteConfirm:
    "Dit maakt een conceptcreditnota die elke regel van deze factuur spiegelt. Snoei hem terug voor een gedeeltelijke creditering en geef hem daarna uit als elk ander document.",
  billingCreditsInvoice: "De factuur die dit crediteert",
  billingFromQuote: "De offerte waar dit uit voortkomt",

  // Betalingen (B1.19): het geld dat op een factuur is binnengekomen. Elk
  // bedrag hier is dat van de server, en "deels betaald" wordt bewust nooit
  // een status genoemd: het document is nog steeds uitgegeven, nog steeds
  // verschuldigd, en nog steeds te laat zodra de datum verstrijkt.
  billingPayments: "Betalingen",
  billingRecordPayment: "Betaling vastleggen",
  billingRecordPaymentHint:
    "Geld dat is binnengekomen. Er wordt niets verstuurd — dit legt alleen vast wat uw bank al laat zien.",
  billingRemovePayment: "Verwijderen",
  billingNoPayments: "Er is nog niets op deze factuur binnengekomen.",
  billingPaidToDate: "Ontvangen",
  billingOutstanding: "Nog verschuldigd",
  billingOverpaidNote:
    "Er is meer ontvangen dan deze factuur waard is. Het verschil kunt u terugbetalen of verrekenen met de volgende.",
  billingPaymentUnpaid: "Onbetaald",
  billingPaymentPartiallyPaid: "Deels betaald",
  billingPaymentPaid: "Voldaan",
  billingColPaidOn: "Ontvangen op",
  billingColMethod: "Hoe",
  billingColPaymentReference: "Bankreferentie",
  billingColAmount: "Bedrag",
  billingFieldAmount: (currency: string) => `Bedrag (${currency})`,
  billingFieldAmountHint:
    "Wat er werkelijk binnenkwam, wat minder kan zijn dan de factuur.",
  billingFieldPaidOn: "Ontvangen op",
  billingFieldPaidOnHint:
    "De dag die uw bank laat zien. Laat leeg voor vandaag.",
  billingFieldMethod: "Hoe het binnenkwam",
  billingFieldMethodHint: "Vrije tekst — hoe uw boekhouding het ook noemt.",
  billingMethodPlaceholder: "Overboeking",
  billingFieldPaymentReference: "Bankreferentie",
  billingFieldPaymentRefHint:
    "De referentie op de afschriftregel, zodat hij later kan worden afgeletterd.",
  billingFilterOverdue: "Achterstallig",
  billingColOutstanding: "Nog verschuldigd",

  // Het btw-overzicht van een periode (B1.20): de bedragen waar een aangifte
  // van wordt overgenomen. De tekst zegt duidelijk welke documenten wel en
  // niet meetellen, omdat iemand juridisch verantwoordelijk is voor wat hij
  // van dit scherm overneemt.
  billingReports: "Btw-overzicht",
  billingReportFrom: "Van",
  billingReportTo: "Tot en met",
  billingReportShow: "Tonen",
  billingReportThisQuarter: "Dit kwartaal",
  billingReportLastQuarter: "Vorig kwartaal",
  billingReportDownloadCsv: "CSV downloaden",
  billingReportDownloadFailed:
    "Het bestand kon niet worden klaargezet. Probeer opnieuw.",
  billingReportBasis: (from: string, to: string) =>
    `Uitgegeven en betaalde documenten gedateerd ${from} tot en met ${to}. Creditnota’s worden afgetrokken; concepten en geannuleerde documenten tellen niet mee.`,
  billingReportColVat: "Btw",
  billingReportTotal: "Totaal",
  billingReportGross: "Inclusief btw",
  billingReportCaption: (currency: string) => `Btw-overzicht in ${currency}`,
  billingReportCounts: (invoices: number, creditNotes: number) =>
    `Uit ${invoices} ${invoices === 1 ? "factuur" : "facturen"} en ${creditNotes} ${
      creditNotes === 1 ? "creditnota" : "creditnota’s"
    }.`,
  billingReportEmptyTitle: "In deze periode is niets uitgegeven",
  billingReportEmptyBody:
    "Een document telt vanaf de dag dat het is uitgegeven. Kies een andere periode, of geef de concepten uit die in deze thuishoren.",

  // Offertes (B1.15): hetzelfde document als een factuur totdat iemand ja
  // zegt, en bewust dezelfde woorden overal waar de twee schermen het eens
  // zijn.
  billingQuotes: "Offertes",
  billingNewQuote: "Nieuwe offerte",
  billingSearchQuotes: "Zoeken op nummer, klant of referentie…",
  billingNoQuotesTitle: "Nog geen offertes",
  billingNoQuotesBody:
    "Bied een klant een prijs. Accepteert hij, dan wordt de offerte een conceptfactuur met dezelfde regels.",
  billingQuoteStatusSent: "Verstuurd",
  billingQuoteStatusAccepted: "Geaccepteerd",
  billingQuoteStatusDeclined: "Afgewezen",
  billingQuoteStatusExpired: "Verlopen",
  billingQuoteLapsed: "Datum verstreken",
  billingColSentDate: "Verstuurd op",
  billingColValidUntil: "Geldig tot",
  billingDraftQuote: "Conceptofferte",
  billingBackToQuotes: "Alle offertes",
  billingQuoteGone: "Deze offerte bestaat niet meer.",
  billingQuoteCustomerHint: "Hun valuta wordt op de offerte overgenomen.",
  billingCreateQuoteHint:
    "Eerst wordt het concept opgemaakt; daarna zet u erbij wat u aanbiedt.",
  billingFieldSentDate: "Verstuurd op",
  billingFieldValidUntil: "Geldig tot",
  billingValidForDays: (days: number) =>
    days === 1
      ? "Staat 1 dag vanaf de dag dat hij wordt verstuurd."
      : `Staat ${days} dagen vanaf de dag dat hij wordt verstuurd.`,
  billingDeleteQuoteDraft: "Concept verwijderen",
  billingDeleteQuoteDraftConfirm:
    "Dit concept verwijderen? Het draagt geen nummer en is nooit aan iemand aangeboden — en er kan niets worden teruggehaald.",
  billingQuoteSentNotice:
    "Deze offerte is verstuurd en kan niet meer worden gewijzigd. Verandert de prijs, maak dan een nieuwe offerte.",
  billingQuoteClosedNotice:
    "Deze offerte is gesloten en kan niet meer worden gewijzigd.",
  billingSendQuote: "Als verstuurd markeren",
  billingSendQuoteTitle: "Deze offerte versturen?",
  billingSendQuoteConfirm:
    "Dit neemt het volgende offertenummer, dateert de offerte en zet de prijzen vast, zodat wat de klant in handen heeft niet onder hem kan veranderen. Er wordt niets gemaild — verstuur hem zelf en leg het hier vast.",
  billingAcceptQuote: "Geaccepteerd",
  billingAcceptQuoteTitle: "Heeft de klant geaccepteerd?",
  billingAcceptQuoteConfirm:
    "Dit sluit de offerte en maakt een conceptfactuur met dezelfde regels tegen dezelfde prijzen. Er wordt nog niets uitgegeven — u komt op het concept uit.",
  billingDeclineQuote: "Afgewezen",
  billingDeclineQuoteTitle: "Heeft de klant afgewezen?",
  billingDeclineQuoteConfirm:
    "De offerte sluit definitief en blijft leesbaar. Van gedachten veranderen is een nieuwe offerte, geen heropende.",
  billingExpireQuote: "Loslaten",
  billingExpireQuoteTitle: "Stoppen met deze offerte?",
  billingExpireQuoteConfirm:
    "De offerte sluit als verlopen, met vandaag als de dag waarop u ermee stopte. Er kan daarna niet meer op worden geantwoord.",
  billingQuoteInvoice: "De factuur die hieruit is voortgekomen",

  // Afdrukken, en de identiteit van de uitgever die elk afgedrukt document
  // draagt (B1.16). Het document zelf wordt door de server opgemaakt en
  // spreekt zijn eigen taaltabel (`billing_print.rs`); dit zijn de woorden
  // eromheen.
  billingPrint: "Afdrukken",
  billingPrintUnsaved:
    "Dit drukt het opgeslagen document af en wacht dus op uw laatste wijziging.",
  billingPrintFailed:
    "Het document kon niet worden klaargezet om af te drukken. Probeer opnieuw.",
  billingSettings: "Uw gegevens",
  billingSettingsIntro:
    "Dit is van wie uw facturen, creditnota’s en offertes komen: de naam en nummers bovenaan, en de rekening waar het geld heen gaat.",
  billingSettingsFirstRun:
    "Vul dit in voordat u iets uitgeeft. Dit staat bovenaan elk document dat u afdrukt, en hier wordt uw klant gevraagd te betalen.",
  billingSettingsIdentity: "Onder welke naam u factureert",
  billingSettingsContact: "Hoe klanten u bereiken",
  billingSettingsBank: "Waar het geld heen gaat",
  billingSettingsFooter: "De regel onder de totalen",
  billingSettingsSaved:
    "Opgeslagen. Elk document dat u vanaf nu afdrukt draagt dit.",
  billingSettingsLoadFailed:
    "Uw facturatiegegevens konden niet worden geladen.",
  billingFieldLegalName: "Statutaire naam",
  billingLegalNameHint:
    "De naam waaronder u handelt en factureert, zoals ingeschreven.",
  billingIssuerVatIdHint:
    "Laat leeg als u niet btw-plichtig bent. Vul eerst uw land in.",
  billingFieldRegistrationNo: "Registratienummer",
  billingRegistrationHint:
    "Zoals uw register het afdrukt — KVK, SIREN, HRB, Companies House.",
  billingFieldPhone: "Telefoon",
  billingFieldWebsite: "Website",
  billingFieldIban: "IBAN",
  billingIbanHint:
    "Vóór het opslaan gecontroleerd op de lengte van uw land en de controlecijfers.",
  billingIbanPlaceholder: "NL91 ABNA 0417 1643 00",
  billingFieldBic: "BIC",
  billingBicPlaceholder: "ABNANL2A",
  billingFieldBankName: "Bank",
  billingFieldAccountHolder: "Rekeninghouder",
  billingAccountHolderHint:
    "Alleen als de rekening niet op uw statutaire naam staat.",
  billingFieldFooterNote: "Voettekst",
  billingFooterNoteHint:
    "Onder de totalen van elk document afgedrukt — eigendomsvoorbehoud, betalingsvoorwaarden, een bedankje.",

  // Meerdere valuta (B1.21). De tekst is nauwkeurig over twee dingen waar
  // iemand juridisch verantwoordelijk voor is: in welke valuta de boeken
  // worden gevoerd, en dat een omgerekend totaal alleen volledig is als elk
  // document erin kon worden omgerekend.
  billingSettingsAccounting: "De valuta waarin u boekhoudt",
  billingFieldBaseCurrency: "Boekhoudvaluta",
  billingBaseCurrencyHint:
    "U kunt in elke valuta factureren. Dit is de valuta waarin uw btw-aangifte wordt gedaan, en waarin de btw op een factuur in vreemde valuta ook wordt afgedrukt.",
  billingFxRates: "Wisselkoersen",
  billingFxIntro:
    "Factureren in een andere valuta vraagt de gepubliceerde koers van de dag van uitgifte. De koersen zijn van u: er wordt niets voor u opgehaald, dus waartegen uw boeken worden omgerekend komt uit een bestand dat u koos.",
  billingFxColDate: "Gepubliceerd",
  billingFxColRate: "Koers per euro",
  billingFxColSource: "Bron",
  billingFxSourceEcb: "Referentiebestand",
  billingFxSourceManual: "Met de hand ingevoerd",
  billingFxAdd: "Een koers toevoegen",
  billingFxAddSaved: (currency: string, date: string) =>
    `De ${currency}-koers voor ${date} is opgeslagen.`,
  billingFxRateHint:
    "Zoals gepubliceerd: eenheden van deze valuta voor één euro, geschreven als 1,1626.",
  billingFxImport: "Een koersbestand importeren",
  billingFxImportHint:
    "Plak de eurofxref-CSV van de Europese Centrale Bank, of elk bestand in die vorm. Een bestand met één foute waarde verandert niets.",
  billingFxImportRun: "Importeren",
  billingFxImported: (rates: number, days: number) =>
    `${rates} koersen over ${days} dagen geïmporteerd.`,
  billingFxEmpty:
    "Nog geen koersen. U hebt ze alleen nodig als u in een andere valuta factureert.",
  billingFxLoadFailed: "De wisselkoersen konden niet worden geladen.",
  billingDocumentFx: (rate: string, day: string) =>
    `Omgerekend tegen ${rate}, de referentiekoers gepubliceerd op ${day}.`,
  billingVatIn: (currency: string) => `Btw in ${currency}`,
  billingReportBaseCaption: (currency: string) => `De periode in ${currency}`,
  billingReportBaseIntro: (currency: string) =>
    `Elk document hierboven, omgerekend tegen de koers die bij uitgifte erop is vastgezet. Hiervan wordt een aangifte in ${currency} gedaan.`,
  billingReportUnconverted: (count: number) =>
    count === 1
      ? "1 document zit niet in deze bedragen: er is geen wisselkoers voor opgeslagen. Controleer het vóór de aangifte."
      : `${count} documenten zitten niet in deze bedragen: er is geen wisselkoers voor opgeslagen. Controleer ze vóór de aangifte.`,

  // Achterstallig geld nabellen (B1.26). De tekst let vooral op één ding: dit
  // schrijft een brief, het verstuurt er geen.
  billingRemind: "Herinneren",
  billingRemindHint:
    "Schrijf een betalingsherinnering aan deze klant en laat hem in uw Concepten staan.",
  billingReminderDrafted: (
    invoice: string,
    outstanding: string,
    days: number,
  ) =>
    days === 1
      ? `Een herinnering voor ${invoice} — ${outstanding} nog verschuldigd, 1 dag over de datum — staat klaar in uw Concepten. Er is niets verstuurd: lees hem, wijzig wat u wilt, en verstuur hem zelf.`
      : `Een herinnering voor ${invoice} — ${outstanding} nog verschuldigd, ${days} dagen over de datum — staat klaar in uw Concepten. Er is niets verstuurd: lees hem, wijzig wat u wilt, en verstuur hem zelf.`,
  billingReminderFailed:
    "De herinnering kon niet worden geschreven. Controleer uw verbinding en probeer opnieuw.",
  billingNothingOverdue:
    "Niets is achterstallig. Elke uitgegeven factuur is voldaan of nog op tijd.",

  // Terugkerende facturen (B2.11). Het woord dat hier overal telt: concept.
  // Een vervaldag maakt een document om te controleren, nooit een uitgegeven
  // factuur.
  billingRecurring: "Terugkerend",
  billingRecurringTitle: "Terugkerende facturen",
  billingRecurringChip: "Terugkerend",
  billingRecurringChipHint:
    "Een terugkerende factuur heeft dit concept aangemaakt.",
  billingNoSchedulesTitle: "Nog geen terugkerende facturen",
  billingNoSchedulesBody:
    "Stel er een in voor alles wat u met een vaste regelmaat factureert — een abonnement, een vast maandbedrag, hosting. Telkens als het zover is, maakt alo een concept dat u zelf controleert en uitgeeft.",
  billingNewSchedule: "Nieuwe terugkerende factuur",
  billingScheduleFrom: "Deze factuur herhalen",
  billingScheduleFromHint:
    "Stel een terugkerende factuur in die deze regels met een vaste regelmaat opnieuw factureert. Elke keer verschijnt als concept — er wordt nooit iets voor u uitgegeven.",
  billingScheduleName: "Naam",
  billingScheduleNameHint:
    "Hoe u deze afspraak noemt. Staat nooit op de factuur.",
  billingScheduleCadence: "Frequentie",
  billingCadenceWeekly: "Elke week",
  billingCadenceMonthly: "Elke maand",
  billingCadenceQuarterly: "Elk kwartaal",
  billingCadenceYearly: "Elk jaar",
  billingScheduleStart: "Eerste op",
  billingScheduleEnd: "Tot",
  billingScheduleEndNever: "Geen einddatum",
  billingScheduleNext: "Volgende",
  billingScheduleLast: "Laatst aangemaakt",
  billingScheduleRaised: "Aangemaakt",
  billingScheduleEach: "Elke keer",
  billingScheduleStatusActive: "Loopt",
  billingScheduleStatusPaused: "Gepauzeerd",
  billingScheduleStatusEnded: "Afgelopen",
  billingScheduleStatusDue: "Aan de beurt",
  billingSchedulePause: "Pauzeren",
  billingScheduleResume: "Hervatten",
  billingScheduleDelete: "Verwijderen",
  billingScheduleDeleteTitle: "Deze terugkerende factuur verwijderen?",
  billingScheduleDeleteMessage:
    "Ze stopt met factureren en verdwijnt uit deze lijst. Alleen een afspraak die nog nooit een concept heeft aangemaakt kan worden verwijderd — pauzeer er een die dat wel deed.",
  billingScheduleRunDue: "Maak aan wat aan de beurt is",
  billingScheduleRunHint:
    "alo doet dit elk uur vanzelf. Dit is er alleen voor als u liever niet wacht.",
  billingScheduleRunNone:
    "Er was niets aan de beurt. Elke terugkerende factuur is bij.",
  billingScheduleRunDrafted: (count: number) =>
    count === 1
      ? "Er is 1 concept aangemaakt en het wacht bij uw facturen. Er is niets uitgegeven: lees het, wijzig wat u wilt, en geef het zelf uit."
      : `Er zijn ${count} concepten aangemaakt en ze wachten bij uw facturen. Er is niets uitgegeven: lees ze, wijzig wat u wilt, en geef ze zelf uit.`,
  billingScheduleSaved: (name: string) =>
    `„${name}” staat klaar. Telkens als het zover is, maakt alo een concept dat u kunt controleren.`,
  billingScheduleAnchorHint: (day: number) =>
    day > 28
      ? `Vastgezet op dag ${day}: in een kortere maand factureert ze op de laatste dag, en in de volgende lange maand weer op dag ${day}.`
      : `Vastgezet op dag ${day} van de maand.`,

  // alo CRM (B2). Een „deal” heet in het Nederlandse zakenleven ook zo; hij
  // schuift per „fase” over een „bord” en sluit gewonnen of verloren.
  moduleCrm: "Verkoop",
  crmBoard: "Bord",
  crmList: "Lijst",
  crmPipeline: "Pijplijn",
  crmDeal: "Deal",
  crmStage: "Fase",
  crmStageArchived: "Gearchiveerde kolom",
  crmLoadFailed: "Uw deals konden niet worden geladen.",
  crmSaveFailed: "De wijziging kon niet worden opgeslagen.",
  crmDeleteFailed: "Dat kon niet worden verwijderd.",
  crmSuggestFailed: "Er konden nu geen gesprekken worden voorgesteld.",
  crmNoBoardTitle: "Nog geen pijplijn",
  crmNoBoardBody:
    "Al uw borden zijn gearchiveerd. Zet er een terug om weer aan deals te werken.",
  crmNoDealsTitle: "Nog geen deals",
  crmNoDealsBody:
    "Maak de eerste kans aan en schuif hem over het bord naarmate hij vordert.",
  crmNoMatches: "Geen enkele deal komt overeen met wat u typte.",

  // Het dealformulier
  crmNewDeal: "Nieuwe deal",
  crmEditDeal: "Deal bewerken",
  crmEdit: "Bewerken",
  crmCreate: "Aanmaken",
  crmSave: "Opslaan",
  crmCancel: "Annuleren",
  crmClose: "Sluiten",
  crmDealSubtitle: "Wat de kans is, met wie hij loopt, en wat hij waard is.",
  crmFieldTitle: "Deal",
  crmFieldCompany: "Bedrijf",
  crmCompanyHint: "Het bedrijf zoals uw hele team het hoort te zien.",
  crmFieldContactName: "Contactpersoon",
  crmFieldContactEmail: "E-mail contactpersoon",
  crmContactEmailHint:
    "Wordt gebruikt om de gesprekken voor te stellen waar deze deal bij hoort.",
  crmFieldValue: "Waarde",
  crmValueHint: "Wat de deal waard is, exclusief btw.",
  crmFieldCurrency: "Valuta",
  crmCurrencyHint: "Drie letters, bijvoorbeeld EUR.",
  crmFieldExpectedClose: "Verwachte afsluiting",
  crmFieldSource: "Herkomst",
  crmSourceHint:
    "Waar de kans vandaan kwam — een aanbeveling, een campagne, een telefoontje.",
  crmNotAnAmount: "Dat is geen bedrag.",
  crmDeleteDeal: "Verwijderen",
  crmDeleteDealConfirm:
    "Dit verwijdert de deal en alles wat erop is vastgelegd. Taken die eruit zijn ontstaan blijven in de lijst van hun eigenaar staan. Dit kan niet ongedaan worden gemaakt.",

  // De lijst
  crmSearchDeals: "Deals zoeken",
  crmFilterStage: "Filteren op fase",
  crmFilterAnyStage: "Alle fasen",
  crmFilterState: "Filteren op status",
  crmFilterAnyState: "Alle statussen",
  crmFilterMine: "Alleen die van mij",
  crmColDeal: "Deal",
  crmColCompany: "Bedrijf",
  crmColStage: "Fase",
  crmColValue: "Waarde",
  crmColExpectedClose: "Verwachte afsluiting",
  crmColState: "Status",
  crmStateOpen: "Open",
  crmStateWon: "Gewonnen",
  crmStateLost: "Verloren",
  crmExpectedClose: (day: string) => `Verwacht ${day}`,
  crmLostBecause: (reason: string) => `Verloren: ${reason}`,

  // Een deal verliezen vraagt waarom: een reden die optioneel is, is een
  // reden die niemand invult — en het gewonnen/verloren-rapport leeft ervan.
  crmLostTitle: "Waarom is hij verloren?",
  crmLostMessage: (stage: string) =>
    `Deze deal naar „${stage}” verplaatsen sluit hem als verloren. Zeg waarom, dan staat de reden in uw gewonnen/verloren-rapport.`,
  crmLostPlaceholder: "Prijs, timing, naar een concurrent…",
  crmLostConfirm: "Als verloren markeren",
  crmLostReasonLabel: "Reden",
  crmLostReasonPrice: "Prijs",
  crmLostReasonTiming: "Timing",
  crmLostReasonCompetitor: "Koos een concurrent",
  crmLostReasonBudget: "Geen budget",
  crmLostReasonNoDecision: "Geen beslissing",
  crmLostReasonNotAFit: "Geen match",

  // Een deal winnen: de overgang naar de facturatie. Beide maken een
  // CONCEPT — er wordt niets uitgegeven, niets verstuurd, en geen
  // factuurnummer opgebruikt.
  crmRaiseQuote: "Offerte",
  crmRaiseInvoice: "Factuur",
  crmDocumentDraft: (kind: string): string =>
    kind === "invoice" ? "conceptfactuur" : "conceptofferte",
  crmRaiseTitle: (document: string) => `Een ${document} aanmaken`,
  crmRaiseSubtitle:
    "Hij komt als concept in Facturatie te staan, om te controleren en aan te vullen. Er wordt niets uitgegeven en niets verstuurd.",
  crmRaiseFrom: (deal: string, value: string) =>
    `Uit „${deal}”, ter waarde van ${value}.`,
  crmRaiseConfirm: "Aanmaken",
  crmRaiseFailed: "Het document kon niet worden aangemaakt.",
  crmFieldVatRate: "Btw-tarief",
  crmVatRateHint:
    "Het tarief waartegen deze regel wordt gefactureerd, in procenten — bijvoorbeeld 21.",
  crmFieldCountry: "Land van de klant",
  crmCountryHint:
    "Twee letters. Deze deal is nog een lead, dus er wordt een klant van gemaakt — en het land bepaalt de btw-behandeling.",
  crmRaisedTitle: (document: string) => `Uw ${document} staat klaar`,
  crmRaisedSubtitle:
    "Open hem in Facturatie om de regels, het adres en de btw te controleren.",
  crmRaisedWorth: (gross: string) => `${gross} inclusief btw.`,
  crmOpenInBilling: "Openen in Facturatie",

  // Het rapport: waarde per fase, en wat er in een periode is gewonnen en
  // verloren. Elk bedrag komt van de server, en twee valuta's worden nooit
  // bij elkaar opgeteld.
  crmReport: "Rapport",
  crmReportFrom: "Van",
  crmReportTo: "Tot",
  crmReportShow: "Tonen",
  crmReportThisQuarter: "Dit kwartaal",
  crmReportLastQuarter: "Vorig kwartaal",
  crmReportDownloadCsv: "CSV downloaden",
  crmReportDownloadFailed: "Het rapport kon niet worden gedownload.",
  crmReportBasis: (from: string, to: string) =>
    `Gewonnen en verloren tussen ${from} en ${to}.`,
  crmReportOpenAsOf: (at: string) =>
    `De open pijplijn is zoals hij ervoor staat op ${at}.`,
  crmReportOpenCaption: (currency: string) =>
    `Open pijplijn per fase (${currency})`,
  crmReportClosedCaption: (currency: string) =>
    `Afgesloten in de periode (${currency})`,
  crmReportColDeals: "Deals",
  crmReportOpenTotal: "Open totaal",
  crmReportWinRate: (rate: string, won: number, closed: number) =>
    `Winstpercentage ${rate} — ${won} van ${closed} afgesloten deals.`,
  crmReportNoWinRate:
    "Er is in deze periode geen deal afgesloten, dus er is geen winstpercentage te tonen.",
  crmReportEmptyTitle: "Nog niets te rapporteren",
  crmReportEmptyBody:
    "Op dit bord staan geen deals. Maak er een aan en hij verschijnt hier, per fase en per valuta.",

  // Het logboek
  crmActivityTitle: "Logboek",
  crmActivityKind: "Soort notitie",
  crmActivityPlaceholder: "Wat er is gezegd of afgesproken…",
  crmActivityAdd: "Vastleggen",
  crmActivityDelete: "Notitie verwijderen",
  crmActivityEmpty: "Er is nog niets vastgelegd.",
  crmKindNote: "Notitie",
  crmKindCall: "Telefoontje",
  crmKindMeeting: "Afspraak",

  // Volgende stappen zijn echte taken, in de lijst die hun eigenaar toch al
  // opent.
  crmNextStepsTitle: "Volgende stappen",
  crmNextStepPlaceholder: "Wat er hierna gebeurt…",
  crmNextStepDue: "Deadline",
  crmNextStepAdd: "Toevoegen",
  crmNextStepsEmpty: "Er is nog geen volgende stap afgesproken.",
  crmOpenInTasks: "Openen in Taken",

  // Gekoppelde gesprekken. E-mail blijft in e-mail: de koppeling is een
  // verwijzing, en alleen een collega die het gesprek al heeft, opent het.
  crmThreadsTitle: "Gesprekken",
  crmThreadsEmpty: "Er is nog geen gesprek gekoppeld.",
  crmThreadSuggest: "Gesprekken voorstellen",
  crmThreadLink: "Koppelen",
  crmThreadUnlink: "Ontkoppelen",
  crmThreadOpenInMail: "Openen in E-mail",
  crmThreadNotYours:
    "Dit gesprek zit niet in uw postvak — vraag de collega die het koppelde.",
  crmThreadLinkedBy: (who: string, when: string) =>
    `Gekoppeld door ${who} · ${when}`,
  crmSuggestionsEmpty:
    "Niets in uw recente e-mail komt overeen met de adressen van deze deal.",
  crmSuggestionAddress: (address: string) => `Komt overeen met ${address}`,
  crmSuggestionDomain: (address: string) => `Zelfde bedrijf als ${address}`,

  // De voorstellen van de agent (ADR 0034): eerst het gedeelde kader, dan de
  // CRM-acties (B2.10). Er gebeurt niets vóór de goedkeuring.
  agentProposedAction: "alo wil dit doen — keur het goed om door te gaan.",
  agentApprove: "Goedkeuren",
  agentDiscard: "Verwerpen",
  agentDone: "Klaar.",
  agentFailed: "Die actie kon niet worden uitgevoerd.",
  agentActCreateDeal: "Nieuwe deal",
  agentActMoveDeal: "Deal verplaatsen",
  agentActFollowup: "Opvolgmail",
  agentFieldDeal: "Deal",
  agentFieldCompany: "Bedrijf",
  agentFieldValue: "Waarde",
  agentFieldStage: "Fase",
  agentFieldLostReason: "Verloren omdat",
  agentDealFromEmailNote: "Koppelt dit gesprek aan de nieuwe deal.",
  agentFollowupNote:
    "Schrijft de e-mail in uw Concepten — er wordt niets verstuurd.",

  // De Projecten-acties (B3.10a, B3.10b). Voorgestelde uren zijn pas uren
  // wanneer degene van wie de urenstaat is ze accepteert: het woord
  // "voorgesteld" staat daarom in elke zin, en de projectstatus zegt met zoveel
  // woorden dat hij alleen leest.
  agentActLogTime: "Uren registreren",
  agentActProjectStatus: "Projectstatus",
  agentFieldProject: "Project",
  agentFieldDay: "Dag",
  agentFieldDuration: "Duur",
  agentLogTimeNote:
    "Stelt een regel voor in uw urenstaat — die telt zodra u hem daar accepteert.",
  agentProjectStatusNote:
    "Leest alleen het project — er wordt niets gewijzigd.",
  // De cijfers van de projectstatus. De server stuurt getallen, nooit een zin:
  // elk woord dat een lezer ziet, staat hier.
  agentTimeLogged: (project: string): string =>
    `Voorgesteld in uw urenstaat op ${project} — accepteer het in Projecten om het te laten meetellen.`,
  agentStatusHours: "Geregistreerde uren",
  agentStatusBillable: (formatted: string): string =>
    `waarvan ${formatted} factureerbaar`,
  agentStatusBudget: "Budget",
  agentStatusBudgetUsed: (percent: string): string => `${percent} verbruikt`,
  agentStatusNoBudget: "Geen urenbudget ingesteld",
  agentStatusInternal: "Intern project — geen klant, geen budget.",
  agentStatusCustomer: "Klant",
  agentStatusMilestones: "Mijlpalen",
  agentStatusMilestonesDone: (done: number, total: number): string =>
    `${done} van ${total} bereikt`,
  agentStatusMilestonesLate: (late: number): string =>
    late === 1 ? "1 te laat" : `${late} te laat`,
  agentStatusNoMilestones: "Geen gepland",
  agentStatusNext: "Volgende",
  agentStatusTasks: "Taken",
  agentStatusTasksOpen: (open: number): string =>
    open === 1 ? "1 open" : `${open} open`,
  agentStatusTasksOverdue: (overdue: number): string =>
    `${overdue} over de datum`,
  agentStatusLastWorked: "Laatst gewerkt",
  agentStatusNeverWorked: "Nog geen uren",
  // Het concept uit de agenda (B3.10b). Een reeks voorstellen, plus wat eruit
  // is gelaten — de server stuurt redencodes, en elk woord ervoor staat hier.
  agentActDraftTimesheet: "Urenstaat uit uw agenda",
  agentDraftTimesheetNote:
    "Stelt één regel voor per afspraak in uw agenda op die dagen — elke regel telt zodra u hem in Projecten accepteert.",
  agentDraftedCount: (count: number): string =>
    count === 1 ? "1 regel voorgesteld" : `${count} regels voorgesteld`,
  agentDraftedNone: "Niets voor te stellen",
  agentDraftedRange: (from: string, to: string): string =>
    from === to ? from : `${from} – ${to}`,
  agentDraftedTotal: "Totaal",
  agentDraftedOverlap: "overlapt de vorige",
  agentDraftedOverlaps: (count: number): string =>
    count === 1
      ? "1 ervan overlapt een andere afspraak — kijk na welke het werk was."
      : `${count} ervan overlappen andere afspraken — kijk na welke het werk waren.`,
  agentDraftedNote: (project: string): string =>
    `Voorgesteld in uw urenstaat op ${project} — accepteer elke regel in Projecten om hem te laten meetellen.`,
  agentDraftedLeftOut: "Eruit gelaten",
  agentDraftedReason: (reason: string): string => {
    switch (reason) {
      case "allDay":
        return "hele dag — geen gewerkte uren";
      case "alreadyDrafted":
        return "staat al in uw urenstaat";
      case "noDuration":
        return "zonder duur";
      case "tooLong":
        return "langer dan een dag";
      case "weekLocked":
        return "die week is ingediend";
      case "limitReached":
        return "boven de reeksgrens — vraag opnieuw voor de resterende dagen";
      case "outsideRange":
        return "begint buiten die dagen";
      default:
        // Een reden die een nieuwere server kent en deze client niet: zeg dat
        // hij eruit is gelaten in plaats van te doen alsof hij is voorgesteld.
        return "eruit gelaten";
    }
  },

  // De geschiedenis van een record (B2.13). Zoals in het Engels: voltooide
  // deelwoorden, want elke regel is iets dat gebeurd is — en de soort record
  // is de pagina die de lezer al open heeft.
  auditHistoryTitle: "Geschiedenis",
  auditHistoryEmpty: "Er is nog niets met dit record gebeurd.",
  auditLoadFailed: "De geschiedenis kon niet worden geladen.",
  auditActionCreate: "Aangemaakt",
  auditActionUpdate: "Bewerkt",
  auditActionDelete: "Verwijderd",
  auditActionArchive: "Gearchiveerd",
  auditActionIssue: "Uitgegeven",
  auditActionVoid: "Geannuleerd",
  auditActionCreditNote: "Creditnota aangemaakt",
  auditActionSend: "E-mail opgesteld",
  auditActionReminder: "Herinnering opgesteld",
  auditActionPaymentCreate: "Betaling vastgelegd",
  auditActionPaymentDelete: "Betaling verwijderd",
  auditActionImport: "Geïmporteerd",
  auditActionSepaXml: "Aan een betaalbestand toegevoegd",
  auditActionApprove: "Goedgekeurd",
  auditActionReject: "Afgekeurd",
  auditActionAccept: "Geaccepteerd",
  auditActionDecline: "Afgewezen",
  auditActionExpire: "Als verlopen gemarkeerd",
  auditActionRun: "Uitgevoerd",
  auditActionPause: "Gepauzeerd",
  auditActionResume: "Hervat",
  auditActionRatesUpdate: "Wisselkoers ingesteld",
  auditActionRatesImport: "Wisselkoersen geïmporteerd",
  auditActionStageMove: "Naar een andere kolom verplaatst",
  auditActionStageCreate: "Kolom toegevoegd",
  auditActionMove: "Verplaatst",
  auditActionQuoteRaised: "Offerte aangemaakt",
  auditActionInvoiceRaised: "Factuur aangemaakt",
  auditActionActivityCreate: "Notitie toegevoegd",
  auditActionNextStepCreate: "Volgende stap toegevoegd",
  auditActionThreadCreate: "Gesprek gekoppeld",
  auditActionThreadDelete: "Gesprek ontkoppeld",
  auditActionLeadCreate: "Leads geïmporteerd",

  // alo Inzichten (ADR 0037, golf BI-1). Een „board” heet hier een dashboard —
  // het woord „bord” is in dit product al bezet door het kanbanbord van Taken
  // en CRM. De zeven vragen van het overzicht dragen exact de titels die de
  // server zaait (`insights_gallery.rs`): dezelfde grafiek heet niet anders
  // omdat ze uit de galerij komt.
  moduleInsights: "Inzichten",
  insightsBoards: "Dashboards",
  insightsLoadFailed: "Uw dashboards konden niet worden geladen.",
  insightsBoardLoadFailed: "Dit dashboard kon niet worden geladen.",
  insightsFiguresFailed: "Deze cijfers konden niet worden gelezen.",
  insightsSaveFailed: "De wijziging kon niet worden opgeslagen.",
  insightsDeleteFailed: "Dat kon niet worden verwijderd.",
  insightsNewBoard: "Nieuw dashboard",
  insightsBoardNamePrompt: "Hoe moet dit dashboard heten?",
  insightsBoardNamePlaceholder: "Kasstroom",
  insightsRenameBoard: "Naam wijzigen",
  insightsDeleteBoard: "Dashboard verwijderen",
  insightsDeleteBoardConfirm: (name: string) =>
    `Het dashboard „${name}” verwijderen? De grafieken gaan mee — de facturen en deals erachter blijven staan.`,
  insightsRefresh: "Cijfers vernieuwen",
  insightsNoBoardsTitle: "Nog geen dashboards",
  insightsNoBoardsBody:
    "Een dashboard bundelt de cijfers die u in één oogopslag wilt zien: wat u factureerde, wat u nog krijgt, wat er in de pipeline zit.",
  insightsNoTilesTitle: "Niets op dit dashboard vastgezet",
  insightsNoTilesBody:
    "Grafieken die u op dit dashboard vastzet, verschijnen hier.",
  insightsAddChart: "Grafiek toevoegen",
  insightsGalleryTitle: "Kant-en-klare grafieken",
  insightsGallerySubtitle:
    "Kies er een om ze op dit dashboard vast te zetten. U kunt ze daarna hernoemen of verwijderen.",
  insightsGalleryClose: "Sluiten",
  insightsGalleryLoadFailed:
    "De kant-en-klare grafieken konden niet worden geladen.",
  insightsGalleryRevenueByMonth: "Omzet per maand",
  insightsGalleryRevenueByMonthBody:
    "Wat u factureerde, maand na maand over het afgelopen jaar — exclusief btw.",
  insightsGalleryOutstanding: "Openstaand",
  insightsGalleryOutstandingBody:
    "Alles wat u nog tegoed hebt op uitgegeven facturen, als één cijfer.",
  insightsGalleryOverdueAging: "Achterstand per ouderdom",
  insightsGalleryOverdueAgingBody:
    "Wat u tegoed hebt, gegroepeerd naar hoe laat het is: 0–30, 31–60, 61–90 en 90+ dagen.",
  insightsGalleryVatByQuarter: "Btw per kwartaal",
  insightsGalleryVatByQuarterBody:
    "De btw die u per kwartaal in rekening bracht — de vorm waarin een aangifte wordt ingediend.",
  insightsGalleryTopCustomers: "Grootste klanten",
  insightsGalleryTopCustomersBody:
    "Waar de omzet van dit jaar vandaan kwam, de tien grootste eerst.",
  insightsGalleryPaymentsByMonth: "Ontvangen betalingen",
  insightsGalleryPaymentsByMonthBody:
    "Geld dat werkelijk binnenkwam, maand na maand, in de valuta waarin het binnenkwam.",
  insightsGalleryPipelineByStage: "Pipeline per fase",
  insightsGalleryPipelineByStageBody:
    "De waarde van open deals in elke kolom van uw trechter.",
  insightsGalleryWonThisMonth: "Gewonnen deze maand",
  insightsGalleryWonThisMonthBody:
    "De waarde van de deals die deze maand als gewonnen sloten.",
  insightsGalleryWinRateByQuarter: "Winstpercentage per kwartaal",
  insightsGalleryWinRateByQuarterBody:
    "Hoe vaak een besliste deal werd gewonnen, kwartaal na kwartaal.",
  insightsGalleryWonByMonth: "Gewonnen per maand",
  insightsGalleryWonByMonthBody:
    "De gewonnen dealwaarde, maand na maand over het afgelopen jaar.",
  insightsAsk: "Vraag om een grafiek",
  insightsAskSubtitle:
    "Beschrijf wat u wilt zien. U krijgt de grafiek eerst te zien — er komt niets op dit dashboard tot u ze vastzet.",
  insightsAskLabel: "Uw vraag",
  insightsAskPlaceholder: "Hoeveel factureerden we elke maand dit jaar?",
  insightsAskSubmit: "Vragen",
  insightsAskClose: "Sluiten",
  insightsAskPreview: "De voorgestelde grafiek",
  insightsAskPin: "Op dit dashboard vastzetten",
  insightsAskDiscard: "Verwerpen",
  insightsAskRepaired:
    "De eerste poging paste niet bij de gegevens en werd gecorrigeerd voor het tekenen.",
  insightsAskFailed: "Uit die vraag kon geen grafiek worden gebouwd.",
  insightsAskUnavailable: "De assistent staat niet aan voor deze werkruimte.",
  insightsTileActions: (title: string) => `Opties voor ${title}`,
  insightsRenameTile: "Grafiek hernoemen",
  insightsRenameTilePrompt: "Hoe moet deze grafiek heten?",
  insightsRemoveTile: "Grafiek verwijderen",
  insightsRemoveTileConfirm: (title: string) =>
    `„${title}” van dit dashboard verwijderen? De records die ze telt, blijven ongemoeid.`,
  insightsWiden: "Breder maken",
  insightsNarrow: "Smaller maken",
  insightsMoveLeft: "Naar voren verplaatsen",
  insightsMoveRight: "Naar achteren verplaatsen",
  insightsUnreadableTitle: "Gemaakt door een nieuwere versie van alo",
  insightsUnreadableBody:
    "De vraag achter deze grafiek is hier niet leesbaar, dus haar cijfers worden niet getoond.",
  insightsNoFigures: "Niets te tonen voor deze periode.",
  insightsTruncated:
    "Alleen de grootste categorieën worden getoond; de rest staat samen onder „Overige”.",
  insightsNoteUnconverted: (count: number) =>
    count === 1
      ? "1 document kon niet in uw boekhoudvaluta worden uitgedrukt en telt niet mee."
      : `${count} documenten konden niet in uw boekhoudvaluta worden uitgedrukt en tellen niet mee.`,
  insightsColBucket: "Categorie",
  insightsColValue: "Waarde",
  insightsBucketTotal: "Totaal",
  insightsBucketOther: "Overige",
  insightsGroupAll: "Alles",
  insightsValueNone: "Geen",
  insightsValueUnknown: "Onbekend",
  insightsStatusIssued: "Uitgegeven",
  insightsStatusPaid: "Betaald",
  insightsOutcomeWon: "Gewonnen",
  insightsOutcomeLost: "Verloren",
  insightsOutcomeOpen: "Open",
  insightsAgeNotDue: "Nog niet vervallen",
  insightsAge0To30: "0–30 dagen",
  insightsAge31To60: "31–60 dagen",
  insightsAge61To90: "61–90 dagen",
  insightsAge90Plus: "90+ dagen",
  // De Nederlandse afkortingen: kwartaal en week, geen Q en W.
  insightsQuarter: (quarter: number, year: number) => `K${quarter} ${year}`,
  insightsWeek: (week: number, year: number) => `W${week} ${year}`,

  // alo Projecten (ADR 0035, golf B3). De woorden van klantwerk: een project
  // dat voor een klant wordt gedaan, de uren die eraan opgaan, de week waarin
  // ze worden ingediend, en de beslissing die iemand over die week neemt.
  //
  // Twee woorden liggen hier vast. Het document dat iemand invult heet een
  // „urenstaat” — niet timesheet, niet urenregistratie, en overal hetzelfde.
  // En het bord van Taken heet ook hier een bord: het ZIJN dezelfde rijen,
  // en dat is precies de bedoeling.
  //
  // Duren staan zoals iemand ze zegt — „7 u 30 min” — nooit als decimale uren:
  // „1,75” op het ene scherm naast „1 u 45 min” op het andere zijn twee
  // getallen die iemand met elkaar moet rijmen.
  moduleProjects: "Projecten",
  projectsTabList: "Projecten",
  projectsTabWeek: "Mijn week",
  projectsTabApprovals: "Goedkeuringen",
  projectsTabReports: "Rapporten",
  projectsTabPlan: "Planning",
  projectsLoadFailed: "Uw projecten zijn niet geladen.",
  projectsSaveFailed: "De wijziging kon niet worden opgeslagen.",
  projectsStartFailed: "De timer kon niet worden gestart.",
  projectsStopFailed: "De timer kon niet worden gestopt.",
  projectsCancel: "Annuleren",
  projectsSave: "Opslaan",
  projectsEdit: "Bewerken",
  projectsActions: "Acties",

  // Duren en tarieven. `projectsNoTime` is het streepje in een lege cel: een
  // blanco cel leest als kapot, een nul als werk dat geen tijd kostte.
  projectsNoTime: "—",
  projectsHoursShort: (hours: number) => `${hours} u`,
  projectsMinutesShort: (minutes: number) => `${minutes} min`,
  projectsPerHour: (amount: string) => `${amount}/u`,
  projectsPercent: (percent: number) => `${percent}%`,
  projectsUnpriced: "Geen tarief",

  // De projectenlijst.
  projectsProject: "Project",
  projectsCustomer: "Klant",
  projectsCustomerHint:
    "De klant aan wie de uren van dit project worden gefactureerd.",
  projectsCustomerPick: "Kies een klant…",
  projectsCustomerUnknown: "Onbekende klant",
  projectsInternal: "Intern",
  projectsRate: "Uurtarief",
  projectsRateHint:
    "Laat u dit leeg, dan tellen de uren wel mee maar krijgen ze geen waarde.",
  projectsRateInvalid: "Schrijf het tarief als bedrag, bijvoorbeeld 95,00.",
  projectsHoursLogged: "Uren",
  projectsBillableHours: "Factureerbaar",
  projectsOfWhichBillable: (duration: string) =>
    `waarvan ${duration} factureerbaar`,
  projectsBudget: "Budget",
  projectsBudgetUsed: "Budget verbruikt",
  projectsBudgetHours: "Budget (uren)",
  projectsBudgetAmount: "Budget (bedrag)",
  projectsBudgetHint: "Richtinggevend. Niets houdt een uur erboven tegen.",
  projectsBudgetHoursInvalid: "Schrijf het budget als een heel aantal uren.",
  projectsBudgetAmountInvalid:
    "Schrijf het budget als bedrag, bijvoorbeeld 7600,00.",
  projectsLastWorked: "Laatst gewerkt",
  projectsNeverWorked: "Nooit",
  projectsStartsOn: "Start op",
  projectsMakeClientWork: "Klantwerk maken",
  projectsStartTimerOn: (project: string) => `Start de timer op ${project}`,
  projectsEmptyTitle: "Nog geen projecten",
  projectsEmptyBody:
    "Een project is hier een bord uit Taken, gezien als klantwerk. Maak er een in Taken en zeg daarna voor wie het wordt gedaan.",

  // Het projectformulier.
  projectsClientSubtitle:
    "Voor wie dit project wordt gedaan, en wat een uur eraan waard is.",
  projectsPersonalBoard:
    "Dit is een persoonlijk bord. Alleen een teamproject kan klantwerk zijn — de uren ervan worden door iemand anders goedgekeurd en aan een klant gefactureerd.",
  projectsDetach: "Intern maken",
  projectsDetachTitle: "Hier intern werk van maken?",
  projectsDetachBody:
    "De uren blijven precies zoals ze zijn. Wat vervalt, is de aanspraak dat ze aan een klant factureerbaar zijn — en uren die al op een factuur staan, blijven op die factuur.",

  // Het weekraster.
  projectsPreviousWeek: "Vorige",
  projectsNextWeek: "Volgende",
  projectsThisWeek: "Deze week",
  projectsWeekOf: (from: string, to: string) => `${from} – ${to}`,
  projectsBillableOf: (hours: string) => `waarvan ${hours} factureerbaar`,
  projectsWeek: "Week",
  projectsDay: "Dag",
  projectsDuration: "Duur",
  projectsDurationHint:
    "90, 1:30 en 1,5 betekenen alle drie anderhalf uur. 2h betekent twee uur.",
  projectsDurationInvalid:
    "Schrijf een duur als 90, 1:30, 1,5 of 2h — hoogstens één dag.",
  projectsTotal: "Totaal",
  projectsAddRow: "Projectregel toevoegen…",
  projectsBillable: "Factureerbaar aan de klant",
  projectsNotBillable: "niet factureerbaar",
  projectsNote: "Notitie",
  projectsNoNote: "Geen notitie",
  projectsNoteHint:
    "Waar u mee bezig was. Niemand buiten deze werkruimte leest het.",
  projectsProposedEntry: "voorgesteld",
  projectsBilledEntry: "op een factuur",
  projectsCellLabel: (project: string, day: string, duration: string) =>
    `${project}, ${day}: ${duration}`,
  projectsDeleteEntry: "Verwijderen",
  projectsDeleteEntryTitle: "Deze uren verwijderen?",
  projectsDeleteEntryBody:
    "De regel verdwijnt voorgoed. Daarvoor moet de week openstaan.",
  projectsWeekEmptyTitle: "Deze week nog niets geregistreerd",
  projectsWeekEmptyBody:
    "Start de timer op een project, of voeg hieronder een regel toe en schrijf de uren rechtstreeks in een dag.",
  projectsBillableOfWeek: (duration: string) =>
    `waarvan ${duration} factureerbaar`,
  projectsProposedInWeek: (duration: string) =>
    `${duration} voorgesteld, nog niet geaccepteerd`,
  // Beslissen over een voorstel (B3.10b). Pas door te accepteren wordt het een
  // uur — de tekst zegt dat, wat „OK” niet zou doen.
  projectsAcceptEntry: "Accepteren",
  projectsRejectEntry: "Verwerpen",
  projectsAcceptEntryLabel: (project: string, duration: string) =>
    `Accepteer de voorgestelde ${duration} op ${project}`,
  projectsRejectEntryLabel: (project: string, duration: string) =>
    `Verwerp de voorgestelde ${duration} op ${project}`,
  projectsSuggestionsWaiting: (count: number) =>
    count === 1
      ? "1 voorstel wacht deze week op u."
      : `${count} voorstellen wachten deze week op u.`,
  projectsSubmitWeek: "Week indienen",
  projectsWithdrawWeek: "Terugnemen",
  projectsRejectedBecause: (note: string) => `Teruggestuurd: ${note}`,

  // De planning — mijlpalen op een tijdlijn, boven het bord dat er al is.
  // „Bereikt” is met opzet een woord van een mens en niet „klaar”: een mijlpaal
  // is bereikt wanneer iemand zegt dat het werk is aanvaard, nooit wanneer de
  // laatste taak eronder is afgesloten.
  projectsPlanLoadFailed: "De planning kon niet worden geladen.",
  projectsMilestoneAdd: "Mijlpaal toevoegen",
  projectsMilestoneNew: "Nieuwe mijlpaal",
  projectsMilestoneName: "Mijlpaal",
  projectsMilestoneNameHint:
    "Waar de datum voor staat — „Ontwerp goedgekeurd”, „Bèta bij de pilotklant”.",
  projectsMilestoneDue: "Datum",
  projectsMilestoneDueHint:
    "De dag waarop het af moet zijn. Hem later zetten is doodgewoon; er wordt niets door tegengehouden.",
  projectsMilestoneReach: "Markeren als bereikt",
  projectsMilestoneReopen: "Nog niet bereikt",
  projectsMilestoneReached: "Bereikt",
  projectsMilestoneLate: "Te laat",
  projectsMilestoneNoTasks: "Nog geen taken eronder",
  projectsMilestoneTasksClosed: (done: number, total: number) =>
    `${done} van ${total} taken afgesloten`,
  projectsMilestoneDelete: "Verwijderen",
  projectsMilestoneDeleteTitle: "Deze mijlpaal verwijderen?",
  projectsMilestoneDeleteBody:
    "De datum verdwijnt; de taken eronder blijven precies staan waar ze op het bord staan.",
  projectsPlanUnplaced: "Niet in de planning",
  projectsPlanPlace: "Zetten onder…",
  projectsPlanPlaceTask: (task: string) => `Zet ${task} onder een mijlpaal`,
  projectsPlanRemove: "Eruit halen",
  projectsPlanEmptyTitle: "Nog geen planning",
  projectsPlanEmptyBody:
    "Een mijlpaal is een datum met een naam op dit project — de data waar een klant naar vraagt. Voeg de eerste toe en zet daarna de taken van het bord eronder.",

  // Sjablonen: een bord dat herbruikbaar is gemarkeerd, en de kopie die eruit
  // begint.
  projectsTemplateNew: "Nieuw uit sjabloon",
  projectsTemplateNewTitle: "Beginnen vanuit een sjabloon",
  projectsTemplateNewSubtitle: "De vorm van het werk, op nieuwe data",
  projectsTemplateCreate: "Project maken",
  projectsTemplateWhich: "Sjabloon",
  projectsTemplateWhichHint:
    "De kaarten, hun kolommen, checklists en labels gaan mee — geen toegewezen personen, opmerkingen, uren of afgeronde kaarten.",
  projectsTemplateOption: (name: string, tasks: number, milestones: number) =>
    `${name} — ${tasks} ${tasks === 1 ? "kaart" : "kaarten"}, ${milestones} ${
      milestones === 1 ? "mijlpaal" : "mijlpalen"
    }`,
  projectsTemplateName: "Naam van het nieuwe project",
  projectsTemplateNameHint: "Hoe dit project op het bord heet.",
  projectsTemplateStarts: "Start op",
  projectsTemplateStartsHint:
    "De eerste mijlpaal van het sjabloon valt op deze dag; alle andere data houden hun onderlinge afstand.",
  projectsTemplateCustomerHint:
    "Een sjabloon is een vorm, geen klant. Laat het leeg voor intern werk; het tarief en het budget gaan hoe dan ook mee.",
  projectsTemplateNoCustomer: "Intern werk",
  projectsTemplateNoPlan:
    "Dit sjabloon heeft geen mijlpalen, dus zijn data worden precies zo gekopieerd.",
  projectsTemplateMarkOn: (project: string) =>
    `Maak van ${project} een sjabloon`,
  projectsTemplateUnmarkOn: (project: string) =>
    `${project} is een sjabloon — markering weghalen`,
  projectsTemplateEmptyTitle: "Nog geen sjablonen",
  projectsTemplateEmptyBody:
    "Open een project dat u nog eens op dezelfde manier zou doen en druk op de ster ernaast. Het blijft een gewoon bord — het kan alleen worden gekopieerd.",
  projectsTemplateFailed: "Dat kon niet worden gedaan.",
  projectsTemplatesLoadFailed: "De sjablonen konden niet worden geladen.",

  // Waar een week staat. Het woord van de server, nooit opnieuw afgeleid in de
  // browser.
  projectsWeekOpen: "Open",
  projectsWeekSubmitted: "Ingediend",
  projectsWeekApproved: "Goedgekeurd",
  projectsWeekRejected: "Teruggestuurd",

  // De goedkeuringenlijst — het enige scherm hier dat een persoon noemt.
  projectsPerson: "Persoon",
  projectsSubmittedAt: "Ingediend op",
  projectsApprove: "Goedkeuren",
  projectsReject: "Terugsturen",
  projectsRejectTitle: "Deze week terugsturen?",
  projectsRejectBody: (person: string) =>
    `${person} leest wat u hier schrijft.`,
  projectsRejectPlaceholder: "Wat er moet worden verbeterd",
  projectsApprovalsEmptyTitle: "Niets goed te keuren",
  projectsApprovalsEmptyBody:
    "Weken die mensen indienen komen hier terecht, de oudste eerst.",

  // Het rentabiliteitsrapport — uren maal tarief, tegenover een budget. Het
  // woord is „waarde” en nooit „marge”: dit is de opbrengstenkant, en wat een
  // uur ons kost vraagt om een grootboek en een personeelsdossier die er geen
  // van beide zijn.
  projectsReportTitle: "Rentabiliteit",
  projectsReportFrom: "Van",
  projectsReportTo: "Tot",
  projectsReportShow: "Tonen",
  projectsReportThisQuarter: "Dit kwartaal",
  projectsReportLastQuarter: "Vorig kwartaal",
  projectsReportDownloadCsv: "CSV downloaden",
  projectsReportDownloadFailed: "Het rapport kon niet worden gedownload.",
  projectsReportBasis: (from: string, to: string) =>
    `Uren gewerkt tussen ${from} en ${to}.`,
  projectsReportBudgetBasis: (to: string) =>
    `Budgetten tellen alles tot en met ${to}, niet alleen deze periode.`,
  projectsReportColValue: "Waarde",
  projectsReportColInvoiced: "Gefactureerd",
  projectsReportColToInvoice: "Te factureren",
  projectsReportColToDate: "Uren tot nu toe",
  projectsReportColBudget: "Budget verbruikt",
  projectsReportTotals: "Alle projecten",
  projectsReportUnrated: (duration: string) => `${duration} zonder tarief`,
  projectsReportUnratedHint:
    "Factureerbare uren zonder tarief. Ze tellen hier mee en krijgen nergens een waarde — geef het project een tarief en registreer ze daarna.",
  projectsReportNoValue: "Nog geen waarde",
  projectsReportBudgetLeft: (amount: string) => `${amount} over`,
  projectsReportBudgetOver: (amount: string) => `${amount} overschreden`,
  projectsReportNoBudget: "Geen budget ingesteld",
  projectsReportEmptyTitle: "Nog geen klantprojecten",
  projectsReportEmptyBody:
    "Rentabiliteit is uren tegenover een tarief en een budget, dus begint het bij een klantproject. Geef een project een klant en een tarief, en dit vult zich.",

  // De lopende timer in de zijbalk.
  projectsTimerRunning: "Timer loopt",
  projectsStopTimer: "Stop de timer",
  projectsStop: "Stoppen",
  mailAttachmentErrorDetail: (reason: string) =>
    `Dat bestand is niet bijgevoegd. Probeer het opnieuw toe te voegen. Server: ${reason}`,
  mailDraftCreateErrorDetail: (reason: string) =>
    `Uw bericht is niet verzonden omdat het concept niet kon worden gemaakt. Het opstelvenster blijft open; probeer opnieuw te verzenden. Server: ${reason}`,
  mailSubmitErrorDetail: (reason: string) =>
    `Uw bericht is niet verzonden. Het blijft in Concepten staan zodat u het kunt openen en opnieuw kunt proberen. Server: ${reason}`,
  mailScheduleErrorDetail: (reason: string) =>
    `Uw bericht is niet gepland. Het blijft in Concepten staan zodat u het kunt openen en opnieuw kunt proberen. Server: ${reason}`,
  driveLoading: "Uw bestanden worden geladen…",
  driveLocations: "Drive-locaties",
  driveFolderLoading: (name: string) => `${name} wordt geladen…`,
  driveFolderLoadFailed: (reason: string) =>
    `Deze map is niet geladen. Server: ${reason}`,
  driveSpacesLoadFailed: (reason: string) =>
    `Uw ruimtes zijn niet geladen. Probeer het opnieuw. Server: ${reason}`,
  driveRetry: "Opnieuw proberen",
  driveUnknownError: "De server gaf geen reden op.",
  driveLoadFailedTitle: "Uw bestanden zijn niet geladen",
  driveLoadFailed: (reason: string) => `Probeer het opnieuw. Server: ${reason}`,
  driveActionFailed: (action: string, reason: string) =>
    `${action} is niet voltooid. Probeer het opnieuw. Server: ${reason}`,
  driveMovedToTrash: (name: string) =>
    `${name} is naar de prullenbak verplaatst.`,
  driveRestoredFromTrash: (name: string) => `${name} is hersteld.`,
  driveUndo: "Ongedaan maken",
  driveSelected: (count: number) =>
    count === 1 ? "1 item geselecteerd" : `${count} items geselecteerd`,
  driveSelectItem: (name: string) => `${name} selecteren`,
  driveSelectAll: "Alle zichtbare items selecteren",
  driveClearSelection: "Selectie wissen",
  driveSelectionActions: "Acties voor geselecteerde items",
  driveItemsMovedToTrash: (count: number) =>
    `${count} items zijn naar de prullenbak verplaatst.`,
  driveItemsRestored: (count: number) => `${count} items zijn hersteld.`,
  drivePurgeManyConfirm: (count: number) =>
    `${count} items permanent verwijderen? Dit kan niet ongedaan worden gemaakt.`,
  driveVersionsLoadFailed: (reason: string) =>
    `De versiegeschiedenis is niet geladen. Probeer het opnieuw. Server: ${reason}`,
  driveMembersLoadFailed: (reason: string) =>
    `De leden zijn niet geladen. Probeer het opnieuw. Server: ${reason}`,
  baseCalendarPreviousMonth: "Vorige maand",
  baseCalendarNextMonth: "Volgende maand",
  baseCalendarAddOnDate: (date: string) => `Een record toevoegen op ${date}`,
  baseLoading: "Uw base wordt geladen…",
  baseBoardEmptyTitle: "Records in een bord groeperen",
  baseCalendarEmptyTitle: "Records op een kalender plaatsen",
  baseBoardEmptyBody:
    "Borden groeperen records via een keuzeveld. Voeg een gebruiksklaar Statusveld toe om door te gaan.",
  baseCalendarEmptyBody:
    "Kalenders plaatsen records via een Datumveld. Voeg er een toe om door te gaan.",
  baseAddStatusField: "Statusveld toevoegen",
  baseAddDateField: "Datumveld toevoegen",
  baseStatusField: "Status",
  baseDateField: "Datum",
  baseStatusTodo: "Te doen",
  baseStatusInProgress: "Bezig",
  baseStatusDone: "Klaar",
  baseLoadFailedTitle: "Deze base is niet geladen",
  baseEmptyTitle: "Begin met uw eerste tabel",
  baseEmptyBody:
    "Tabellen houden verwante records bij elkaar. Maak er een om velden en records toe te voegen.",
  baseDefaultTableName: (number: number) => `Tabel ${number}`,
  baseView: "Weergave",
  baseSaveChanges: "Wijzigingen opslaan",
  officeLoading: "De Office-editor wordt geopend…",
  officeDiscoveryMissing:
    "De Office-editor heeft geen editoradres gepubliceerd.",
  officeLoadFailed: (reason: string) =>
    `Probeer het opnieuw. Server: ${reason}`,
  sheetLoading: "Uw sheet wordt geladen…",
  sheetLoadFailedTitle: "Deze sheet is niet geladen",
  docLoading: "Uw document wordt geladen…",
  docLoadFailedTitle: "Dit document is niet geladen",
  docSaveFailed: (reason: string) =>
    `Uw laatste wijzigingen zijn nog niet opgeslagen. Kies Opnieuw proberen om ze op te slaan. Server: ${reason}`,
  sheetSaveFailed: (reason: string) =>
    `Uw laatste wijzigingen zijn nog niet opgeslagen. We blijven het proberen. Server: ${reason}`,
  sitesSubmissions: "Inzendingen",
  sitesSubmissionsLoadFailed:
    "Uw formulierinzendingen konden niet worden geladen.",
  sitesSubmissionSaveFailed: "Deze inzending kon niet worden bijgewerkt.",
  sitesNoSubmissionsTitle: "Nog geen berichten",
  sitesNoSubmissionsBody:
    "Voeg een contactformulier aan een pagina toe. Nieuwe bezoekersberichten verschijnen hier.",
  sitesOpenPages: "Pagina’s openen",
  sitesSubmissionList: "Bezoekersberichten",
  sitesSubmissionDetail: "Geselecteerd bezoekersbericht",
  sitesHandled: "Afgehandeld",
  sitesNeedsReply: "Antwoord nodig",
  sitesMarkHandled: "Markeren als afgehandeld",
  sitesReopenSubmission: "Heropenen",
  sitesForm: "Formulier",
  sitesReceived: "Ontvangen",
  sitesExportSubmissions: "CSV exporteren",
  sitesExportingSubmissions: "Export voorbereiden…",
  sitesSubmissionsExportFailed:
    "Uw inzendingen konden niet worden geëxporteerd. Probeer het opnieuw.",
  sitesAnalytics: "Statistieken",
  sitesAnalyticsLoadFailed:
    "De statistieken van uw site konden niet worden geladen. Probeer het opnieuw.",
  sitesAnalyticsLoading: "Sitestatistieken laden",
  sitesAnalyticsPeriod: "Periode voor statistieken",
  sitesAnalyticsDays: (days: number) => `${days} dagen`,
  sitesAnalyticsSummary: "Verkeersoverzicht",
  sitesAnalyticsVisits: "Bezoeken",
  sitesAnalyticsVisitors: "Dagelijkse bezoekers",
  sitesAnalyticsOverTime: "Bezoeken door de tijd",
  sitesAnalyticsChartLabel: "Dagelijkse sitebezoeken",
  sitesAnalyticsDayLabel: (date: string, visits: number) =>
    `${date}: ${visits} ${visits === 1 ? "bezoek" : "bezoeken"}`,
  sitesAnalyticsTopPages: "Populairste pagina’s",
  sitesAnalyticsTopReferrers: "Belangrijkste verwijzers",
  sitesAnalyticsDirect: "Rechtstreeks",
  sitesAnalyticsPrivacyTitle: "Geen cookies. Geen banner.",
  sitesAnalyticsPrivacyBody:
    "Verkeer wordt per dag anoniem geteld. alo bewaart geen bezoekersadres, apparaatprofiel of browsegeschiedenis.",
  sitesAnalyticsEmptyTitle: "Nog geen bezoeken",
  sitesAnalyticsEmptyBody:
    "Open of deel uw gepubliceerde site. De eerste bezoeken verschijnen hier automatisch.",
  sitesAnalyticsOpenSite: "Live site openen",
  sitesAnalyticsPrivacyBeacon:
    "Leestijd en uitgaande kliks worden gemeld door een klein script op uw pagina’s. Het draagt geen enkele identiteit mee, dus twee meldingen van dezelfde browser zijn niet aan elkaar te koppelen.",
  // Sites — de gegroepeerde detailpanelen (S2.08b).
  sitesAnalyticsGroupArrival: "Hoe men u vond",
  sitesAnalyticsGroupPages: "Wat men bekeek",
  sitesAnalyticsGroupReading: "Hoe men las",
  sitesAnalyticsShowAll: (count: number) => `Alle ${count} tonen`,
  sitesAnalyticsShowTop: (count: number) => `Top ${count} tonen`,
  sitesAnalyticsReferrersNote:
    "De site vanwaar een bezoeker een link volgde. Alleen het domein wordt bewaard, nooit de pagina.",
  sitesAnalyticsReferrersEmpty:
    "Nog geen verwijzers. Ze verschijnen zodra een andere site naar de uwe linkt.",
  sitesAnalyticsCampaigns: "Campagnes",
  sitesAnalyticsCampaignsNote:
    "Gelezen uit utm_campaign in de links die u deelt, zodat u een nieuwsbrief van een affiche kunt onderscheiden.",
  sitesAnalyticsCampaignsEmpty:
    "Nog geen campagnes. Voeg ?utm_campaign=lentemailing toe aan een link die u deelt en de bezoeken worden hier geteld.",
  sitesAnalyticsNoCampaign: "Zonder campagne",
  sitesAnalyticsCountries: "Landen",
  sitesAnalyticsCountriesNote:
    "Bepaald door het netwerk vóór uw site, nooit uit een bewaard bezoekersadres.",
  sitesAnalyticsCountriesEmpty:
    "Geen landen gemeld. Uw site wordt geserveerd zonder netwerk dat ze benoemt: dit paneel blijft leeg, alle andere cijfers blijven volledig.",
  sitesAnalyticsNotReported: "Niet gemeld",
  sitesAnalyticsTopPagesNote: "De pagina’s die het vaakst zijn geopend.",
  sitesAnalyticsPagesEmpty: "Nog geen pagina’s geteld in deze periode.",
  sitesAnalyticsEntryPages: "Eerste pagina’s",
  sitesAnalyticsEntryPagesNote:
    "De pagina waarmee de dag van een bezoeker op uw site begon.",
  sitesAnalyticsExitPages: "Laatste pagina’s",
  sitesAnalyticsExitPagesNote:
    "De laatste pagina die die dag is gezien. Daar eindigde het lezen, niet noodzakelijk waar iemand afhaakte.",
  sitesAnalyticsReadTime: "Leestijd",
  sitesAnalyticsReadTimeNote:
    "Hoelang pagina’s in beeld bleven, voor de hele site en niet per pagina. Alleen browsers die het melden tellen mee, dus deze aantallen halen uw bezoekcijfer nooit.",
  sitesAnalyticsReadTimeEmpty:
    "Nog geen leestijden. Ze komen binnen zodra bezoekers uw gepubliceerde pagina’s openen in een browser die dit meldt.",
  sitesAnalyticsReadUnder10s: "Minder dan 10 seconden",
  sitesAnalyticsRead10to30s: "10 tot 30 seconden",
  sitesAnalyticsRead30to60s: "30 tot 60 seconden",
  sitesAnalyticsRead1to3m: "1 tot 3 minuten",
  sitesAnalyticsRead3to10m: "3 tot 10 minuten",
  sitesAnalyticsReadOver10m: "Meer dan 10 minuten",
  sitesAnalyticsOutbound: "Uitgaande links",
  sitesAnalyticsOutboundNote:
    "Domeinen waarnaar bezoekers vertrokken. Voorbij 200 bestemmingen per dag worden de overige samen geteld.",
  sitesAnalyticsOutboundEmpty:
    "Nog geen uitgaande kliks. Ze worden geteld wanneer een bezoeker een link naar een andere site volgt.",
  sitesAnalyticsOutboundOther: "Andere domeinen",
  sitesAnalyticsDevices: "Apparaten",
  sitesAnalyticsDevicesNote:
    "Een grove klasse, afgeleid uit wat de browser over zichzelf zegt. Meer wordt er niet van bewaard.",
  sitesAnalyticsDevicesEmpty: "Nog geen apparaten geteld in deze periode.",
  sitesAnalyticsDevicePhone: "Telefoon",
  sitesAnalyticsDeviceTablet: "Tablet",
  sitesAnalyticsDeviceDesktop: "Computer",
  sitesAnalyticsDeviceBot: "Bots en crawlers",
  sitesAnalyticsDeviceUnknown: "Niet herkend",
  // Sites — de aandachtskaart (S2.09b).
  sitesHeatmap: "Aandachtskaart",
  sitesBackToAnalytics: "Terug naar statistieken",
  sitesHeatmapLoadFailed:
    "De aandachtskaart kon niet worden geladen. Probeer het opnieuw.",
  sitesHeatmapLoading: "Aandachtskaart laden",
  sitesHeatmapPage: "Pagina",
  sitesHeatmapPageOption: (path: string, events: number) =>
    `${path} — ${events} geteld`,
  sitesHeatmapScreens: "Schermformaat",
  sitesHeatmapScreenTab: (screen: string, events: string) =>
    `${screen} (${events})`,
  sitesHeatmapPrivacyTitle: "Een vorm, geen opname.",
  sitesHeatmapPrivacyBody:
    "Kliks en leesdiepte worden per zone van de pagina geteld, per dag. Geen muisspoor, geen sessieopname, en niets wat twee bezoeken aan dezelfde persoon kan koppelen.",
  sitesHeatmapPrivacyShape:
    "Alleen browsers die het melden worden geteld, en hoogstens twintig kliks per paginaweergave. Lees dit als waar de aandacht heen ging — nooit als hoeveel mensen iets deden.",
  sitesHeatmapEmptyTitle: "Nog niets om in kaart te brengen",
  sitesHeatmapEmptyBody:
    "Kliks en leesdiepte verschijnen hier zodra bezoekers uw gepubliceerde pagina’s openen. U hoeft niets aan te zetten.",
  sitesHeatmapClicks: "Waar men klikte",
  sitesHeatmapClicksNote:
    "De hele pagina, van boven naar beneden, niet één scherm. Een donkerder vakje is een zone waarop meer werd geklikt.",
  sitesHeatmapClicksLabel: (path: string, screen: string, clicks: number) =>
    `Kaart van waar ${clicks} kliks landden op ${path}, op ${screen}`,
  sitesHeatmapTop: "Bovenkant van de pagina",
  sitesHeatmapBottom: "Onderkant van de pagina",
  sitesHeatmapLegendQuiet: "Rustiger",
  sitesHeatmapLegendBusy: "Drukker",
  sitesHeatmapLeft: "Links",
  sitesHeatmapCentre: "Midden",
  sitesHeatmapRight: "Rechts",
  sitesHeatmapSpot: (side: string, band: string) => `${side}, ${band}`,
  sitesHeatmapDepthBand: (from: number, to: number) =>
    `${from}–${to}% naar beneden`,
  sitesHeatmapSpots: "Drukste zones",
  sitesHeatmapSpotsNote:
    "Dezelfde kaart in woorden, zodat ze zonder de kleuren te lezen is.",
  sitesHeatmapClicksEmpty:
    "Er is niets geklikt op deze pagina op dit schermformaat.",
  sitesHeatmapSpotsEmpty: "Nog niets te beschrijven.",
  sitesHeatmapSpotsHeldBack:
    "Wordt getoond zodra er genoeg kliks geteld zijn om te beschrijven.",
  sitesHeatmapDepth: "Hoe ver men las",
  sitesHeatmapDepthNote:
    "Hoeveel lezers elk tiende van de pagina bereikten. Alleen browsers die het melden tellen mee, dus dit komt nooit op uw aantal bezoeken uit.",
  sitesHeatmapDepthEmpty: "Hier geen leesdiepte geteld op dit schermformaat.",
  sitesHeatmapTooFewTitle: "Te weinig om een kaart te tekenen",
  sitesHeatmapTooFewClicks: (collected: number, needed: number) =>
    `${collected} van ${needed} kliks geteld op dit schermformaat. Een kaart uit een handvol kliks toont dat handvol, niet uw bezoekers — daarom wordt ze pas getoond als er genoeg zijn.`,
  sitesHeatmapTooFewDepth: (collected: number, needed: number) =>
    `${collected} van ${needed} leesmeldingen geteld op dit schermformaat. De curve verschijnt zodra er genoeg zijn om iets te betekenen.`,
  // Sites — wat de site heeft opgeleverd (S2.10c): van paginaweergave tot
  // factuur, via de CRM/Facturatie-naad uit S2.10b.
  sitesFunnel: "Resultaten",
  sitesFunnelPeriod: "Periode",
  sitesFunnelLoading: "Resultaten laden",
  sitesFunnelLoadFailed:
    "De resultaten konden niet worden geladen. Probeer het opnieuw.",
  sitesFunnelDeniedTitle: "Buiten uw toegang",
  sitesFunnelDeniedFallback:
    "Deze pagina leest alo CRM en alo Facturatie, die niet open staan voor dit account.",
  sitesFunnelDeniedWay:
    "Al het andere van deze site — de pagina’s, de aanvragen en het verkeer — blijft van u.",
  sitesFunnelNoSourcesTitle: "Nog geen contactformulier",
  sitesFunnelNoSourcesBody:
    "Zet een contactformulier op een pagina, dan kunt u elke aanvraag volgen van de eerste paginaweergave tot de factuur.",
  sitesFunnelChain: "Van bezoeker tot factuur",
  sitesFunnelStageViews: "Zagen het formulier",
  sitesFunnelStageStarts: "Begonnen te typen",
  sitesFunnelStageSubmits: "Aanvragen",
  sitesFunnelStageLeads: "Doorgegeven aan verkoop",
  sitesFunnelStageWon: "Gewonnen",
  sitesFunnelStageInvoices: "Facturen",
  sitesFunnelFromBrowser: "Gemeld door de browser",
  sitesFunnelFromRecord: "Geteld bij het opslaan",
  sitesFunnelFloorNote:
    "De eerste twee stappen worden gemeld door de browser van de bezoeker, en een browser die niets meldt heeft de pagina toch gezien. Vanaf de aanvraag telt alles op het moment dat het record werd geschreven. Lees deze getallen als een ondergrens: een verhouding over die grens heen is het laagst mogelijke, geen meting.",
  sitesFunnelMoney: "Het geld erachter",
  sitesFunnelInvoiceRule:
    "Facturen opgemaakt voor de klant die een aanvraag werd, ná het doorgeven.",
  sitesFunnelMoneyEmpty:
    "Er is nog geen kans aangemaakt vanuit deze website.",
  sitesFunnelOpen: "In behandeling",
  sitesFunnelWon: "Gewonnen",
  sitesFunnelInvoiced: "Gefactureerd",
  sitesFunnelHidden: "Niet getoond",
  sitesFunnelBillingOff:
    "Factuurbedragen worden niet getoond omdat alo Facturatie niet open staat voor dit account. Dat is iets anders dan dat er niets is gefactureerd.",
  sitesFunnelCurrencies:
    "Twee valuta’s zijn twee regels en geen totaal: een prognose heeft geen factuurdatum om op om te rekenen.",
  sitesFunnelSources: "Per contactformulier",
  sitesFunnelColSource: "Contactformulier",
  sitesFunnelColDeals: "Kansen",
  sitesFunnelDealsSummary: (open: number, won: number, lost: number) =>
    `${open} in behandeling · ${won} gewonnen · ${lost} verloren`,
  sitesFunnelSumNote:
    "Eén factuur die vanuit twee formulieren bereikbaar is, telt één keer voor de site en één keer onder elk formulier. Deze kolommen zijn dus een lezing per formulier en tellen niet op tot de totalen hierboven.",
  sitesFunnelDeletedSource: "Verwijderd formulier",
  // Sites — één aanvraag doorgeven aan het verkoopbord (S2.10c).
  sitesHandoffSection: "Verkoop",
  sitesHandoffInvite:
    "Maak van deze aanvraag een kans op uw verkoopbord. Niets op dit scherm hoeft opnieuw te worden getypt.",
  sitesHandoffTitle: "Deze aanvraag doorgeven aan verkoop",
  sitesHandoffSubtitle:
    "Maakt een kans op uw verkoopbord en koppelt die aan deze aanvraag.",
  sitesHandoffSubmit: "Doorgeven aan verkoop",
  sitesHandoffFrom: "Van",
  sitesHandoffCarried:
    "De naam, het adres en het bericht gaan mee met het doorgeven — u typt ze nooit opnieuw.",
  sitesHandoffTitleFor: (who: string) => `Aanvraag via de website — ${who}`,
  sitesHandoffBoard: "Bord",
  sitesHandoffColumn: "Kolom",
  sitesHandoffCardTitle: "Kans",
  sitesHandoffValue: "Verwachte waarde",
  sitesHandoffValueHint: "Optioneel — wat u denkt dat het waard is.",
  sitesHandoffCurrency: "Valuta",
  sitesHandoffCurrencyHint: "Laat leeg voor de valuta van uw werkruimte.",
  sitesHandoffLoadingBoards: "Uw verkoopborden laden…",
  sitesHandoffNoBoards:
    "Er is nog geen verkoopbord om dit aan door te geven. Open alo CRM één keer, dan wordt uw eerste bord voor u aangemaakt.",
  sitesHandoffCrmDenied: "alo CRM staat niet open voor dit account.",
  sitesHandoffBoardsFailed:
    "Uw verkoopborden konden niet worden geladen. Probeer het opnieuw.",
  sitesHandoffFailed:
    "Deze aanvraag kon niet worden doorgegeven. Probeer het opnieuw.",
  sitesInSales: "Bij verkoop",
  sitesLeadsLoadFailed:
    "De verkoopkoppelingen voor deze inbox konden niet worden geladen.",
  sitesLeadStanding: (state: string, value: string) => `${state} · ${value}`,
  sitesLeadOpen: "In behandeling",
  sitesLeadWon: "Gewonnen",
  sitesLeadLost: "Verloren",
  sitesUnlinkLead: "Ontkoppelen",
  sitesUnlinkLeadFailed:
    "De koppeling kon niet worden verwijderd. De kans zelf blijft ongemoeid. Probeer het opnieuw.",
  // Sites — de geschiedenis van gepubliceerde versies (S2.04b).
  sitesHistory: "Versiegeschiedenis",
  sitesHistorySubtitle:
    "Elke versie van deze site die u hebt gepubliceerd. Bekijk er een, en zet er een met één klik weer online.",
  sitesHistoryLoadFailed: "De versiegeschiedenis kon niet worden geladen.",
  sitesHistoryVersions: "Gepubliceerde versies",
  sitesHistoryLiveNow: "Nu online",
  sitesHistoryVersionOf: (date: string) => `Versie van ${date}`,
  sitesHistoryPagesCount: (pages: number) =>
    `${pages} ${pages === 1 ? "pagina" : "pagina's"}`,
  sitesHistoryLanguages: (languages: string) => `Talen: ${languages}`,
  sitesHistoryRestoredCopy: (date: string) =>
    `Een kopie van de versie van ${date}`,
  sitesHistoryRestore: "Deze versie weer online zetten",
  sitesHistoryRestoring: "Weer online zetten…",
  sitesHistoryRestoreFailed:
    "Deze versie kon niet weer online worden gezet.",
  sitesHistoryRestored: (date: string) =>
    `De versie van ${date} staat weer online.`,
  sitesHistoryUndo: "Ongedaan maken",
  sitesHistoryUndone: (date: string) =>
    `Terug naar de versie van ${date}. Er is niets verloren — alle versies staan er nog.`,
  sitesHistoryPage: "Pagina",
  sitesHistoryPreviewLoadFailed: "Deze versie kon niet worden getoond.",
  sitesHistoryPreviewLoading: "Deze versie laden",
  sitesHistoryPreviewTitle: "Voorbeeld van gepubliceerde versie",
  sitesHistoryDraftSafe:
    "Uw werk in uitvoering blijft onaangeroerd: een versie weer online zetten verandert nooit wat u aan het bewerken bent.",
  sitesHistoryIfRestored: "Als u deze versie weer online zet",
  sitesHistoryIdentical: "Dit is precies wat nu online staat.",
  sitesHistoryThemeChange: "Het uiterlijk van de site zou veranderen.",
  sitesHistoryLanguagesBack: (languages: string) =>
    `Deze talen zouden terugkomen: ${languages}`,
  sitesHistoryLanguagesGone: (languages: string) =>
    `Deze talen zouden verdwijnen: ${languages}`,
  sitesHistoryPageBack: (page: string) => `${page} zou terugkomen`,
  sitesHistoryPageGone: (page: string) => `${page} zou verdwijnen`,
  sitesHistoryPageChanged: (page: string) => `${page} zou veranderen`,
  sitesHistoryUnchangedPages: (pages: number) =>
    `${pages} ${pages === 1 ? "pagina blijft" : "pagina's blijven"} hetzelfde`,
  sitesHistoryEmptyTitle: "Nog niets gepubliceerd",
  sitesHistoryEmptyBody:
    "Publiceer deze site één keer, en elke versie die u publiceert blijft hier — om terug te kijken, en om weer online te zetten.",

  // Sites — publiceren op een gekozen moment (S2.05b).
  sitesScheduleTitle: "Publiceren op een gekozen moment",
  sitesScheduleHint:
    "Kies een datum en tijd, dan gaat deze site vanzelf online. U hoeft er niet bij te zijn.",
  sitesScheduleLoading: "Controleren wat er gepland staat",
  sitesScheduleLoadFailed:
    "De geplande publicatie kon niet worden geladen.",
  sitesScheduleOpen: "Publicatie plannen",
  sitesScheduleChange: "Moment wijzigen",
  sitesScheduleWhen: "Datum en tijd",
  sitesScheduleGoesLive: (moment: string) => `Gaat online op ${moment}.`,
  sitesScheduleTimeZone: (zone: string) =>
    `Dat is uw eigen tijd (${zone}), niet die van de server.`,
  sitesScheduleSave: "Publicatie plannen",
  sitesScheduleMove: "Naar dit moment verplaatsen",
  sitesScheduleSaving: "Opslaan…",
  sitesScheduleMissingMoment: "Kies eerst een datum en tijd.",
  sitesScheduleSaveFailed: "Deze site kon niet worden ingepland.",
  sitesSchedulePending: (moment: string) =>
    `Deze site publiceert zichzelf op ${moment}. Alles wat u tot dan opslaat, gaat mee online.`,
  sitesSchedulePublishingNow: "Deze site wordt nu gepubliceerd.",
  sitesScheduleCancel: "Afzeggen",
  sitesScheduleCancelling: "Afzeggen…",
  sitesScheduleCancelFailed:
    "De geplande publicatie kon niet worden afgezegd.",
  sitesScheduleCancelled: (moment: string) =>
    `Afgezegd. Deze site wordt niet gepubliceerd op ${moment}, en er is niets veranderd aan wat online staat.`,
  sitesScheduleDone: (moment: string) =>
    `Deze site heeft zichzelf gepubliceerd op ${moment}.`,
  sitesScheduleFailed: (moment: string, reason: string) =>
    `Deze site kon niet worden gepubliceerd op ${moment}: ${reason}`,

  // Sites — een pagina achter een wachtwoord (S2.06b).
  sitesPagePasswordTitle: "Wie deze pagina kan openen",
  sitesPagePasswordLoading: "Nagaan wie deze pagina kan openen",
  sitesPagePasswordLoadFailed:
    "Er kon niet worden nagegaan of deze pagina om een wachtwoord vraagt.",
  sitesPagePasswordUnknown:
    "Het is op dit moment niet bekend of deze pagina bezoekers om een wachtwoord vraagt.",
  sitesPagePasswordPublic: "Iedereen op internet kan deze pagina openen.",
  sitesPagePasswordPublicHint:
    "Geef ze een wachtwoord en alleen de mensen aan wie u het geeft, kunnen ze lezen. De rest van deze website blijft openbaar.",
  sitesPagePasswordProtected: (moment: string) =>
    `Alleen wie het wachtwoord heeft, kan deze pagina openen — ingesteld op ${moment}.`,
  sitesPagePasswordProtectedUndated:
    "Alleen wie het wachtwoord heeft, kan deze pagina openen.",
  sitesPagePasswordProtectedHint:
    "Alle anderen komen op een ontgrendelscherm dat niets van de pagina toont, zelfs de titel niet. Het wachtwoord houdt ze de rest van de dag open.",
  sitesPagePasswordEveryLanguage:
    "Dit geldt voor de pagina in elke taal waarin ze gepubliceerd is.",
  sitesPagePasswordProtect: "Deze pagina beveiligen",
  sitesPagePasswordChange: "Wachtwoord wijzigen",
  sitesPagePasswordField: "Wachtwoord",
  sitesPagePasswordFieldHint:
    "Niemand kan het u later teruglezen, wij ook niet — een vergeten wachtwoord wordt vervangen, niet teruggehaald.",
  sitesPagePasswordEffective:
    "Het werkt meteen. U hoeft de website niet opnieuw te publiceren.",
  sitesPagePasswordShow: "Tonen",
  sitesPagePasswordHide: "Verbergen",
  sitesPagePasswordSaving: "Opslaan…",
  sitesPagePasswordMissing: "Typ eerst een wachtwoord.",
  sitesPagePasswordSaveFailed: "Deze pagina kon niet worden beveiligd.",
  sitesPagePasswordSaved:
    "Opgeslagen. Bezoekers hebben vanaf nu dit wachtwoord nodig, en wie de pagina met het oude had geopend, wordt opnieuw gevraagd.",
  sitesPagePasswordRemove: "Wachtwoord verwijderen",
  sitesPagePasswordRemoveConfirm: "Ja, openbaar maken",
  sitesPagePasswordRemoveFailed: "Het wachtwoord kon niet worden verwijderd.",
  sitesPagePasswordRemoved:
    "Het wachtwoord is weg. Iedereen op internet kan deze pagina weer openen.",
  sitesPagePasswordPreviewNote:
    "Bezoekers wordt eerst om het wachtwoord gevraagd. Deze voorvertoning toont de pagina zoals iemand die het heeft ze ziet.",
  sitesPagePasswordBadge: "Wachtwoord",
  sitesPosts: "Blogartikelen",
  sitesBackToWebsite: "Website",
  sitesPostsLoadFailed: "Uw blogartikelen konden niet worden geladen.",
  sitesLoadingPosts: "Blogartikelen laden",
  sitesWriteInDocs: "Schrijven in alo Docs",
  sitesOpeningDocs: "alo Docs openen…",
  sitesUntitledArticle: "Naamloos artikel",
  sitesPostCreateFailed:
    "Het artikel kon niet worden gemaakt. Probeer het opnieuw.",
  sitesNoPostsTitle: "Nog geen artikelen",
  sitesNoPostsBody:
    "Begin een artikel in alo Docs. Het blijft privé totdat u het publiceert.",
  sitesColArticle: "Artikel",
  sitesColUpdated: "Bijgewerkt",
  sitesColActions: "Acties",
  sitesEditInDocs: "Bewerken in alo Docs",
  sitesPostStatusDraft: "Concept",
  sitesPostStatusPublished: "Gepubliceerd",
  sitesPublishArticle: "Publiceren",
  sitesPublishArticleTitle: "Artikel publiceren",
  sitesPublishArticleSubtitle:
    "Kies hoe het artikel op uw openbare website wordt weergegeven.",
  sitesEditArticleTitle: "Artikelgegevens",
  sitesEditArticleSubtitle: "Werk bij wat lezers op uw website zien.",
  sitesEditArticleDetails: "Gegevens bewerken",
  sitesSaveArticle: "Wijzigingen opslaan",
  sitesPostSaveFailed:
    "De artikelgegevens konden niet worden opgeslagen. Probeer het opnieuw.",
  sitesPostUnpublishFailed:
    "Het artikel kon niet offline worden gehaald. Probeer het opnieuw.",
  sitesUnpublishArticle: "Offline halen",
  sitesUnpublishingArticle: "Offline halen…",
  sitesFieldPostTitle: "Artikeltitel",
  sitesFieldPostSlug: "Webadres",
  sitesPostSlugHint: "Kleine letters, cijfers en koppeltekens.",
  sitesPostSlugPlaceholder: "mijn-artikel",
  sitesFieldPostExcerpt: "Samenvatting",
  sitesPostExcerptHint: "Een korte inleiding voor de blogpagina en RSS-feed.",
  sitesFieldPostCover: "Omslagafbeelding",
  sitesPostCoverHint:
    "Wordt op de blogpagina en boven het artikel weergegeven.",
  sitesPostNoCover: "Geen omslag",
  sitesPostCoverAdded: "Omslag toegevoegd",
  sitesAddPostCover: "Afbeelding toevoegen",
  sitesReplacePostCover: "Afbeelding vervangen",
  sitesRemovePostCover: "Verwijderen",
  sitesUploadingPostCover: "Uploaden…",
  sitesPostCoverUploadFailed:
    "De omslagafbeelding kon niet worden geüpload. Probeer het opnieuw.",
  sitesSeoAction: "Zoeken en delen",
  sitesSeoTitle: "Zoeken en delen",
  sitesSeoSubtitle:
    "Kies hoe deze pagina in zoekresultaten en gedeelde links verschijnt.",
  sitesSeoPreview: "Voorbeeld van zoekresultaat",
  sitesSeoFieldTitle: "Zoektitel",
  sitesSeoTitleHint: "Laat leeg om de paginatitel en websitenaam te gebruiken.",
  sitesSeoFieldDescription: "Beschrijving",
  sitesSeoDescriptionHint:
    "Een korte, nuttige samenvatting voor zoeken en gedeelde links.",
  sitesSeoDescriptionDefault:
    "Voeg een beschrijving toe zodat mensen weten waar deze pagina over gaat.",
  sitesSeoImageHint:
    "Gedeelde links gebruiken eerst de hero-afbeelding en daarna uw sitelogo.",
  sitesSeoSave: "Zoekgegevens opslaan",
  sitesSeoSaveFailed:
    "De zoekgegevens konden niet worden opgeslagen. Probeer het opnieuw.",
  sitesStartingPoint: "Hoe wilt u beginnen",
  sitesGenerateChoice: "Genereren vanuit een beschrijving",
  sitesTemplateChoice: "Beginnen met een sjabloon",
  sitesBusinessDescription: "Beschrijf uw bedrijf",
  sitesBusinessDescriptionHint:
    "Vertel wat u aanbiedt, voor wie het is en welke uitstraling u wilt. U kunt alles bewerken voordat u publiceert.",
  sitesBusinessDescriptionPlaceholder:
    "Een buurtbakkerij met zuurdesembrood en feesttaarten voor lokale gezinnen…",
  sitesGenerateSite: "Website genereren",
  sitesGenerating: "Uw concept wordt voorbereid…",
  sitesCreatingSite: "Website maken…",
  sitesGenerationFailed:
    "Uw concept kon niet worden voorbereid. Bekijk het serverbericht en probeer opnieuw.",
  sitesGenerationEmpty:
    "Het gegenereerde concept bevatte geen pagina. Probeer een uitgebreidere beschrijving.",
  sitesGenerationUnavailable:
    "Genereren is niet ingesteld voor deze werkruimte. Begin met een lege site of kies hieronder een sjabloon.",
  sitesChooseTemplate: "Kies een beginpunt",
  sitesBlankTemplate: "Lege site",
  sitesBlankTemplateSummary:
    "Een lege startpagina. U kiest zelf elke sectie.",
  sitesTemplatePageCount: (count: number) =>
    count === 1 ? "1 pagina" : `${count} pagina’s`,
  sitesTemplatesLoading: "De sjablonen worden geladen…",
  sitesTemplatesLoadFailed:
    "De sjablonen konden niet worden geladen. U kunt nog steeds met een lege site beginnen.",
  sitesTemplatePreviewTitle: (name: string) => `Voorbeeld van ${name}`,
  sitesTemplatePreviewPages: "Pagina’s in dit sjabloon",
  sitesTemplatePreviewLoading: "Het voorbeeld wordt geladen…",
  sitesTemplatePreviewFailed:
    "Dit voorbeeld kon niet worden geladen. U kunt de website nog steeds met dit sjabloon maken.",
  sitesTemplatePreviewNote:
    "Een afbeelding van de pagina. Wissel hierboven van pagina; elk woord en elke sectie past u daarna zelf aan.",
  sitesBlankPreviewNote:
    "U begint met een lege startpagina en voegt zelf de secties toe die u wilt.",
  sitesHomePageTitle: "Home",
  sitesAiEditTitle: "Beschrijf een paginawijziging",
  sitesAiEditBody:
    "alo maakt een controleerbare lijst. Er verandert niets tot u goedkeurt.",
  sitesAiInstruction: "Paginawijziging",
  sitesAiInstructionPlaceholder:
    "Maak het welkom warmer en zet ervaringen boven de prijzen…",
  sitesAiPropose: "Wijzigingen voorbereiden",
  sitesAiPreparing: "Wijzigingen voorbereiden…",
  sitesAiProposalTitle: "Voorgestelde wijzigingen",
  sitesAiProposalCount: (count: number) =>
    count === 1
      ? "1 voorgestelde wijziging"
      : `${count} voorgestelde wijzigingen`,
  sitesAiPreviewHint:
    "Vergelijk de pagina voor en na en kies daarna wat er gebeurt.",
  sitesAiPreviewCompare: "Voorgestelde paginawijzigingen vergelijken",
  sitesInlineTextHint:
    "Klik op een tekst in het voorbeeld om die daar te bewerken. Enter bewaart, Escape zet terug.",
  sitesInlineTextSaved: "Tekst bijgewerkt.",
  sitesInlineTextUndone: "Tekstwijziging ongedaan gemaakt.",
  sitesInlineTextRedone: "Tekstwijziging opnieuw uitgevoerd.",
  sitesInlineTextStale:
    "Die tekst hoort bij een sectie die intussen is verplaatst of gewijzigd. Het voorbeeld is ververst — probeer de bewerking opnieuw.",
  sitesUndoEdit: "Laatste wijziging ongedaan maken",
  sitesRedoEdit: "Laatste wijziging opnieuw uitvoeren",
  sitesSectionDragHint:
    "Sleep een sectie om die te verplaatsen — de pagina schikt zich meteen opnieuw. Met het toetsenbord: selecteer een sectie en houd Alt ingedrukt met de pijl omhoog of omlaag.",
  sitesSectionResizeHint:
    "Sommige secties kunnen van vorm veranderen. Kies een formaat onder de sectie in de lijst, of geef de sectie focus in het voorbeeld en houd Alt ingedrukt met de pijl naar links of rechts.",
  sitesLayoutOf: (control: string) => `Kies ${control.toLowerCase()}`,
  sitesSectionResized: (section: string, choice: string) =>
    `${section} ingesteld op ${choice.toLowerCase()}.`,
  sitesLayoutSplit: "Verdeling",
  sitesLayoutColumns: "Kolommen",
  sitesLayoutShape: "Vorm",
  sitesLayoutSplitWideImage: "Bredere afbeelding",
  sitesLayoutSplitHalf: "Gelijke helften",
  sitesLayoutSplitWideText: "Bredere tekst",
  sitesLayoutColumnsTwo: "Twee",
  sitesLayoutColumnsThree: "Drie",
  sitesLayoutColumnsFour: "Vier",
  sitesLayoutShapeNatural: "Zoals geüpload",
  sitesLayoutShapeWide: "Breed",
  sitesLayoutShapeSquare: "Vierkant",
  sitesLayoutShapeTall: "Hoog",
  sitesSectionOnPage: (section: string, position: number, total: number) =>
    `${section}, sectie ${position} van ${total}. Sleep de sectie om die te verplaatsen, of houd Alt ingedrukt en gebruik de pijl omhoog of omlaag.`,
  sitesAiPreviewBefore: "Voor",
  sitesAiPreviewAfter: "Na",
  sitesAiApprove: "Wijzigingen goedkeuren",
  sitesAiApplying: "Wijzigingen toepassen…",
  sitesAiDiscard: "Verwerpen",
  sitesAiEditFailed:
    "De wijzigingslijst kon niet worden voorbereid. Probeer opnieuw of bewerk de secties direct.",
  sitesAiApplyFailed:
    "Deze wijzigingen konden niet worden toegepast. Bekijk het serverbericht en probeer opnieuw.",
  sitesAiAddChange: (section: string, position: number) =>
    `${section} toevoegen op positie ${position}`,
  sitesAiRemoveChange: (section: string) => `${section} verwijderen`,
  sitesAiMoveChange: (section: string, position: number) =>
    `${section} verplaatsen naar positie ${position}`,
  sitesAiSettingChange: (section: string) =>
    `Een instelling in ${section} bijwerken`,
  sitesAiCopyChange: (section: string) => `Tekst in ${section} herschrijven`,
  sitesAiImproveCopy: "Deze tekst verbeteren",
  sitesAiCopyActions: "Tekstverbeteringen",
  sitesAiRewrite: "Herschrijven",
  sitesAiShorter: "Korter maken",
  sitesAiLonger: "Details toevoegen",
  sitesAiTone: "Gewenste toon",
  sitesAiTonePlaceholder: "Warm en direct",
  sitesAiUseTone: "Toon wijzigen",
  sitesAiCopyBefore: "Huidige tekst",
  sitesAiCopyAfter: "Voorgestelde tekst",
  sitesAiCopyFailed:
    "Deze tekstwijziging kon niet worden voorbereid. Probeer opnieuw of blijf de tekst direct bewerken.",
  sitesLoadFailed: "Uw websites konden niet worden geladen.",
  sitesSiteLoadFailed: "Deze website kon niet worden geladen.",
  sitesSaveFailed: "De wijziging kon niet worden opgeslagen.",
  sitesCheckFailed: "Het adres kon niet worden gecontroleerd.",
  sitesNewSite: "Nieuwe website",
  sitesNoSitesTitle: "Nog geen websites",
  sitesNoSitesBody:
    "Bouw een website voor uw bedrijf en publiceer die op een eigen adres.",
  sitesColName: "Naam",
  sitesColAddress: "Adres",
  sitesColStatus: "Status",
  sitesStatusDraft: "Concept",
  sitesStatusLive: "Online",
  sitesNewSiteTitle: "Nieuwe website",
  sitesNewSiteSubtitle:
    "Begin met een beschrijving of kies een van de kant-en-klare sjablonen.",
  sitesFieldName: "Websitenaam",
  sitesFieldSubdomain: "Adres",
  sitesSubdomainHint:
    "Kleine letters, cijfers en koppeltekens, 3–40 tekens — dit wordt het webadres van de site.",
  sitesSubdomainChecking: "Beschikbaarheid controleren…",
  sitesSubdomainAvailable: (subdomain: string) =>
    `‘${subdomain}’ is beschikbaar.`,
  sitesSubdomainTaken: (subdomain: string) =>
    `‘${subdomain}’ is al in gebruik.`,
  sitesCreateSite: "Website maken",
  sitesCancel: "Annuleren",
  sitesBack: "Alle websites",
  sitesCollaborators: "Medewerkers",
  sitesCollaboratorsHint:
    "Nodig mensen uit om deze website te bewerken en te publiceren. Ze kunnen je mail, bestanden of andere websites niet openen.",
  sitesCollaboratorEmail: "E-mailadres",
  sitesCollaboratorEmailPlaceholder: "medewerker@voorbeeld.nl",
  sitesInviteCollaborator: "Editor uitnodigen",
  sitesCollaboratorsLoading: "Medewerkers laden…",
  sitesCollaboratorsLoadFailed: "De medewerkers van deze website konden niet worden geladen.",
  sitesCollaboratorInviteFailed: "De medewerker kon niet worden uitgenodigd.",
  sitesCollaboratorRevokeFailed: "De toegang van deze medewerker kon niet worden verwijderd.",
  sitesCollaboratorCopyFailed: "De instellink kon niet worden gekopieerd. Maak een nieuwe link en probeer opnieuw.",
  sitesCollaboratorLinkReady: (email: string) =>
    `Er staat een persoonlijke instellink klaar voor ${email}. Kopieer en deel hem veilig.`,
  sitesCollaboratorAdded: (email: string) => `${email} kan deze website nu bewerken.`,
  sitesCollaboratorLinkCopied: "Instellink gekopieerd.",
  sitesCollaboratorRevoked: (email: string) => `De toegang van ${email} is verwijderd.`,
  sitesUndoCollaboratorRevoke: "Ongedaan maken",
  sitesNoCollaborators:
    "Alleen jij kunt deze website bewerken. Vul hierboven een e-mailadres in om de eerste medewerker uit te nodigen.",
  sitesCollaboratorPending: "Uitnodiging in afwachting",
  sitesCollaboratorActive: "Kan bewerken en publiceren",
  sitesRefreshCollaboratorLink: "Nieuwe instellink",
  sitesCopyCollaboratorLink: "Instellink kopiëren",
  sitesRevokeCollaborator: "Toegang verwijderen",
  sitesInvitationHeading: "Deze website bewerken",
  sitesInvitationSubtitle: (site: string) =>
    `Je bent uitgenodigd om ${site} te bewerken en te publiceren.`,
  sitesInvitationLoading: "Je uitnodiging controleren…",
  sitesInvitationLoadFailed:
    "Deze uitnodiging is verlopen of al gebruikt. Vraag de website-eigenaar om een nieuwe link.",
  sitesInvitationPassword: "Maak een wachtwoord",
  sitesInvitationPasswordHint: "Gebruik minstens 8 tekens.",
  sitesInvitationConfirmPassword: "Bevestig wachtwoord",
  sitesInvitationPasswordMismatch: "De wachtwoorden komen niet overeen.",
  sitesInvitationAccept: "Website openen",
  sitesInvitationAccepting: "Bezig met openen…",
  sitesInvitationAcceptFailed: "Je uitnodiging kon niet worden geaccepteerd.",
  sitesInvitationDone: "Je kunt aan de slag",
  sitesInvitationDoneBody: (email: string) =>
    `Meld je aan als ${email}. Je ziet alleen de websites die met je zijn gedeeld.`,
  sitesInvitationSignIn: "Aanmelden bij alo",
  sitesPages: "Pagina’s",
  sitesNewPage: "Nieuwe pagina",
  sitesNoPagesTitle: "Nog geen pagina’s",
  sitesNoPagesBody:
    "Elke site begint met een homepage. Voeg er een toe om te beginnen.",
  sitesColPage: "Pagina",
  sitesColPath: "Pad",
  sitesHomeBadge: "Home",
  sitesNewPageTitle: "Nieuwe pagina",
  sitesNewPageSubtitle: "Een pagina bevat de secties die u erop stapelt.",
  sitesFieldPageTitle: "Titel",
  sitesFieldSlug: "Pad",
  sitesLanguagesLabel: "Websitetalen",
  sitesEditingLanguage: "Bewerkingstaal",
  sitesLanguages: "Talen",
  sitesLanguagesHint:
    "Voeg de talen toe die bezoekers kunnen kiezen en zie welke pagina’s nog vertaald moeten worden.",
  sitesDefaultLanguage: "Standaardtaal",
  sitesAddLanguage: "Taal toevoegen",
  sitesLanguagePlaceholder: "Taalcode, bijvoorbeeld nl",
  sitesAddLanguageAction: "Taal toevoegen",
  sitesLanguageDefaultBadge: "Standaard",
  sitesRemoveLanguage: (language: string) => `${language} verwijderen`,
  sitesLanguageSaveFailed:
    "De websitetalen konden niet worden opgeslagen. Controleer de taalcode en probeer opnieuw.",
  sitesTranslationReady: "Gereed",
  sitesTranslationProgress: (translated: number, total: number) =>
    `${translated} van ${total} pagina’s vertaald`,
  sitesTranslationAllReady:
    "Elke ingeschakelde taal is klaar om te publiceren.",
  sitesTranslationPublishHint: (count: number) =>
    `${count} ${count === 1 ? "vertaling gebruikt" : "vertalingen gebruiken"} nog reservetekst.`,
  sitesContinueTranslating: "Doorgaan met vertalen",
  sitesTranslationSaveFailed:
    "Deze vertaling kon niet worden opgeslagen. Corrigeer de gemarkeerde gegevens en probeer opnieuw.",
  sitesTranslationMissingTitle: (locale: string) =>
    `${locale} heeft een vertaling nodig`,
  sitesTranslationMissingBody: (requested: string, source: string) =>
    `Je ziet de versie ${source} als voorbeeld. Kopieer deze naar ${requested} om te vertalen zonder de bronpagina te wijzigen.`,
  sitesCopyTranslation: (source: string, target: string) =>
    `${source} naar ${target} kopiëren`,
  sitesTranslationDetails: "Gegevens van vertaalde pagina",
  sitesTranslationDetailsHint: (locale: string) =>
    `Deze titel, dit pad en deze zoekgegevens worden alleen aan bezoekers met taal ${locale} getoond.`,
  sitesSaveTranslation: "Vertaalgegevens opslaan",
  sitesSlugHint:
    "Kleine letters, cijfers en koppeltekens. De homepage laat dit veld leeg.",
  sitesFieldHome: "Dit is de homepage",
  sitesCreatePage: "Pagina maken",
  sitesPageLoadFailed: "Deze pagina kon niet worden geladen.",
  sitesBackToSite: "Alle pagina’s",
  sitesSections: "Secties",
  sitesAddSection: "Sectie toevoegen",
  sitesNoSectionsTitle: "Deze pagina is nog leeg",
  sitesNoSectionsBody:
    "Stapel secties — een blikvanger, uw voordelen, een contactformulier — om de pagina te bouwen.",
  sitesPickerTitle: "Sectie toevoegen",
  sitesPickerSubtitle: "Kies een blok en vul daarna de inhoud in.",
  sitesAddSectionTitle: (section: string) => `${section} toevoegen`,
  sitesEditSectionTitle: (section: string) => `${section} bewerken`,
  sitesSaveSection: "Sectie opslaan",
  sitesMoveUp: (section: string) => `${section} omhoog`,
  sitesMoveDown: (section: string) => `${section} omlaag`,
  sitesEditSection: (section: string) => `${section} bewerken`,
  sitesDeleteSection: (section: string) => `${section} verwijderen`,
  sitesSectionMoved: (section: string, position: number, total: number) =>
    `${section} verplaatst naar positie ${position} van ${total}.`,
  sitesConfirmDelete: "Deze sectie verwijderen?",
  sitesPreview: "Voorbeeld",
  sitesPreviewTitle: "Conceptvoorbeeld",
  sitesPreviewDesktop: "Desktopbreedte",
  sitesPreviewMobile: "Telefoonbreedte",
  sitesPreviewFailed: "Het voorbeeld kon niet worden geladen.",
  sitesSectionNav: "Navigatiebalk",
  sitesSectionNavDesc: "Links bovenaan de pagina.",
  sitesSectionHero: "Blikvanger",
  sitesSectionHeroDesc: "De grote openingstitel.",
  sitesSectionFeatures: "Voordelen",
  sitesSectionFeaturesDesc: "Een raster van wat u aanbiedt.",
  sitesSectionTextImage: "Tekst en afbeelding",
  sitesSectionTextImageDesc: "Een alinea naast een afbeelding.",
  sitesSectionGallery: "Galerij",
  sitesSectionGalleryDesc: "Een wand met afbeeldingen.",
  sitesSectionTestimonials: "Ervaringen",
  sitesSectionTestimonialsDesc: "Woorden van tevreden klanten.",
  sitesSectionPricing: "Prijzen",
  sitesSectionPricingDesc: "Uw pakketten en hun prijzen.",
  sitesSectionTeam: "Team",
  sitesSectionTeamDesc: "De mensen achter het bedrijf.",
  sitesSectionFaq: "Veelgestelde vragen",
  sitesSectionFaqDesc: "Veelgestelde vragen met antwoorden.",
  sitesSectionCta: "Oproep tot actie",
  sitesSectionCtaDesc: "Een banner die uitnodigt om te klikken.",
  sitesSectionContactForm: "Contactformulier",
  sitesSectionContactFormDesc: "Laat bezoekers u schrijven.",
  sitesSectionFooter: "Voettekst",
  sitesSectionFooterDesc: "De regel onderaan de pagina.",
  sitesCountLinks: (count: number) =>
    count === 1 ? "1 link" : `${count} links`,
  sitesCountImages: (count: number) =>
    count === 1 ? "1 afbeelding" : `${count} afbeeldingen`,
  sitesCountEntries: (count: number) =>
    count === 1 ? "1 item" : `${count} items`,
  sitesItemN: (position: number) => `Item ${position}`,
  sitesRemoveItem: "Item verwijderen",
  sitesAddLink: "Link toevoegen",
  sitesAddEntry: "Item toevoegen",
  sitesAddImage: "Afbeelding toevoegen",
  sitesAddTier: "Pakket toevoegen",
  sitesAddMember: "Persoon toevoegen",
  sitesAddQuestion: "Vraag toevoegen",
  sitesFieldHeading: "Kop",
  sitesFieldSubheading: "Subkop",
  sitesFieldIntro: "Inleiding",
  sitesFieldBody: "Tekst",
  sitesFieldItemTitle: "Titel",
  sitesFieldLinkLabel: "Linktekst",
  sitesFieldLinkHref: "Linkdoel",
  sitesFieldButton: "Knop",
  sitesFieldPrimaryButton: "Primaire knop",
  sitesFieldSecondaryButton: "Secundaire knop",
  sitesFieldImage: "Afbeelding",
  sitesFieldPhoto: "Foto",
  sitesFieldImageId: "Afbeeldings-ID",
  sitesImageIdHint:
    "Upload een afbeelding of plak een afbeeldings-ID van een eerdere upload.",
  sitesFieldImageAlt: "Afbeeldingsbeschrijving",
  sitesImageAltHint:
    "Wordt voorgelezen door schermlezers. Zeg wat de afbeelding toont; toont ze niets wat ertoe doet, vink dan hieronder ‘decoratief’ aan.",
  sitesImageAltMissing:
    "Deze afbeelding heeft nog geen beschrijving — zeg wat ze toont, of markeer ze als decoratief.",
  sitesImageDecorative: "Decoratief — schermlezers slaan de afbeelding over",
  sitesImageDecorativeHint:
    "Alleen voor afbeeldingen die zelf geen informatie dragen, zoals een achtergrondpatroon.",
  sitesImageFrameHint:
    "Sleep over de afbeelding om te kiezen wat zichtbaar blijft. Met het toetsenbord: pijltoetsen verplaatsen het kader, shift met de pijltoetsen maakt het groter of kleiner.",
  sitesImageFocalHint:
    "Sleep het ronde punt naar wat in beeld moet blijven wanneer een lay-out de afbeelding verder bijsnijdt.",
  sitesImageFrameAt: (width: number, height: number, left: number, top: number) =>
    `Zichtbaar gebied: ${width}% bij ${height}% van de afbeelding, ${left}% vanaf links en ${top}% vanaf boven`,
  sitesImageFocalAt: (x: number, y: number) => `Aandachtspunt ${x}% horizontaal en ${y}% verticaal`,
  sitesImageFrameWidth: "Breedte",
  sitesImageFrameHeight: "Hoogte",
  sitesImageFrameLeft: "Links",
  sitesImageFrameTop: "Boven",
  sitesImageWholePicture: "De hele afbeelding gebruiken",
  sitesImageWholePictureState: "De hele afbeelding is te zien",
  sitesImageCentreFocal: "Aandachtspunt centreren",
  sitesImageNoPreview:
    "Deze afbeelding kan hier niet worden getoond. De waarden hieronder kadreren ze nog steeds en de beschrijving blijft ongewijzigd.",
  sitesAiAltWrite: "Een beschrijving voorstellen",
  sitesAiAltImprove: "Deze beschrijving verbeteren",
  sitesAiAltProposed: "Voorgestelde beschrijving",
  sitesAiAltUnseen:
    "Opgesteld uit de tekst van deze sectie — alo heeft de afbeelding niet gezien. Controleer ze aan de afbeelding voordat u goedkeurt.",
  sitesAiAltFailed: "De beschrijving kon niet worden opgesteld.",
  sitesFieldImageSide: "Kant van de afbeelding",
  sitesSideLeft: "Links",
  sitesSideRight: "Rechts",
  sitesFieldQuote: "Citaat",
  sitesFieldAuthor: "Auteur",
  sitesFieldRole: "Functie",
  sitesFieldTierName: "Pakketnaam",
  sitesFieldPrice: "Prijs",
  sitesFieldPeriod: "Factureringsperiode",
  sitesFieldTierDescription: "Beschrijving",
  sitesFieldTierFeatures: "Wat is inbegrepen",
  sitesTierFeaturesHint: "Eén regel per punt.",
  sitesFieldHighlighted: "Dit pakket uitlichten",
  sitesFieldMemberName: "Naam",
  sitesFieldBio: "Biografie",
  sitesFieldQuestion: "Vraag",
  sitesFieldAnswer: "Antwoord",
  sitesFieldSuccessMessage: "Bericht na verzenden",
  sitesFieldFooterText: "Voettekst",
  sitesContactFormHint:
    "Het formulier staat al op de pagina; verzenden werkt zodra formulieren beschikbaar zijn.",
  sitesTheme: "Thema",
  sitesThemeTitle: "Websitethema",
  sitesThemeSubtitle: "Kies een stijl en voeg uw logo en favicon toe.",
  sitesThemeApply: "Thema toepassen",
  sitesThemeLoadFailed: "De thema-opties konden niet worden geladen.",
  sitesThemePresets: "Kleuren en lettertypen",
  sitesThemeLogo: "Logo",
  sitesThemeLogoHint:
    "Wordt in de navigatiebalk getoond in plaats van de sitenaam.",
  sitesThemeFavicon: "Favicon",
  sitesThemeFaviconHint:
    "Het kleine pictogram dat browsers op het tabblad tonen.",
  sitesThemeUpload: "Afbeelding uploaden",
  sitesThemeReplace: "Afbeelding vervangen",
  sitesThemeRemove: "Afbeelding verwijderen",
  sitesThemeSet: "Afbeelding geüpload",
  sitesThemeNotSet: "Nog geen",
  sitesUploadFailed: "De afbeelding kon niet worden geüpload.",
  sitesUploadImage: "Afbeelding uploaden",
  sitesPublish: "Publiceren",
  sitesPublishChanges: "Wijzigingen publiceren",
  sitesUnpublish: "Offline halen",
  sitesConfirmUnpublish: "De site echt offline halen?",
  sitesLiveAtLabel: "Uw site staat online op",
  sitesGoesLiveAt: (address: string) =>
    `Publiceren zet deze site online op ${address}.`,
  sitesAddressPreview: (address: string) =>
    `Uw site komt online op ${address}.`,
  sitesPublishFailed: "De site kon niet worden gepubliceerd.",
  sitesUnpublishFailed: "De site kon niet offline worden gehaald.",

  // ---- alo Financiën (wave B4, vertaald bij B4.15) ------------------------
  //
  // De woorden zijn die van de documenten die deze schermen maken: een
  // *declaratie*, een *rekeningafschrift*, een *rekeningschema*, een
  // *btw-aangifte* — niet een omschrijving van het Engelse woord. Twee vaste
  // keuzes: *afletteren* is het werkwoord voor het bankwerk (wat een
  // boekhouder zegt, waar "matchen" een leenwoord blijft), en *uitgegeven* is
  // de status van een factuur, zoals in het facturatiescherm (B1.27), zodat
  // dezelfde factuur in twee modules niet twee namen draagt.
  moduleFinance: "Financiën",
  financeTabExpenses: "Declaraties",
  financeTabApprovals: "Goedkeuringen",
  financeLoadFailed: "Uw declaraties konden niet worden geladen.",
  financeSaveFailed: "De wijziging kon niet worden opgeslagen.",
  financeCancel: "Annuleren",
  financeSave: "Opslaan",
  financeEdit: "Bewerken",
  financeDelete: "Verwijderen",
  financeActions: "Acties",
  financeShow: "Tonen",
  financeFrom: "Van",
  financeTo: "Tot",

  // De declaratie zelf.
  financeNewClaim: "Nieuwe declaratie",
  financeEditClaim: "Declaratie bewerken",
  financeClaimSubtitle: "Wat u hebt uitgegeven, en wiens geld betaalde.",
  financeSpentOn: "Datum",
  financeSpentOnHint: "De dag waarop het geld wegging, in uw eigen tijdzone.",
  financeMerchant: "Leverancier",
  financeMerchantHint: "Wie er betaald is — de naam op de bon.",
  financeNoMerchant: "Geen leverancier",
  financeClaimOf: (merchant: string, day: string) => `${merchant}, ${day}`,
  financeDescription: "Waarvoor het was",
  financeGross: "Totaal",
  financeVat: "Btw",
  financeVatHint: "De btw die op de bon staat. Laat leeg als er geen op staat.",
  financeNoVat: "—",
  financeVatRate: "Btw-tarief %",
  financeVatRateHint: "Zoals afgedrukt: 19, 21, 5,5.",
  financeCurrency: "Valuta",
  financeCurrencyHint: "Laat leeg voor de eigen valuta van uw werkruimte.",
  financeProject: "Project",
  financeProjectHint:
    "Koppel de declaratie aan klantwerk, zodat ze in de kosten van dat project verschijnt.",
  financeNoProject: "Geen project",
  financeMethod: "Betaald met",
  financeMethodHint: "Alleen uw eigen geld eindigt in een terugbetaling.",
  financeMethodPersonal: "Eigen geld",
  financeMethodCard: "Bedrijfskaart",
  financeMethodCash: "Kleine kas",
  financeMethodPersonalOption: "Mijn eigen geld",
  financeMethodCardOption: "De bedrijfskaart",
  financeMethodCashOption: "De kleine kas",
  financeAmountInvalid: "Dat is geen bedrag.",
  financeRateInvalid: "Dat is geen percentage.",

  // Waar een declaratie staat. Het woord van de server, in de taal van de
  // persoon.
  financeStatus: "Status",
  financeAnyStatus: "Elke status",
  financeStatusDraft: "Concept",
  financeStatusSubmitted: "In afwachting",
  financeStatusApproved: "Goedgekeurd",
  financeStatusRejected: "Afgewezen",
  financeStatusReimbursed: "Terugbetaald",
  financePaidBackOn: (day: string) => `Terugbetaald op ${day}`,

  // De werkwoorden.
  financeSubmit: "Indienen",
  financeWithdraw: "Intrekken",
  financeApprove: "Goedkeuren",
  financeReject: "Afwijzen",
  financeMarkPaidBack: "Markeren als terugbetaald",
  financeMarkPaidBackSubtitle: (person: string, amount: string) =>
    `${amount} terug naar ${person}.`,
  financeReimbursedOn: "Terugbetaald op",
  financeReimbursedOnHint:
    "De dag waarop het geld echt is verplaatst — op die dag wordt het geboekt.",
  financeDeleteTitle: "Deze declaratie verwijderen?",
  financeDeleteBody:
    "De declaratie en alles wat u erin hebt getypt verdwijnen. Dit kan niet ongedaan worden gemaakt.",
  financeRejectTitle: "Deze declaratie afwijzen",
  financeRejectBody: (person: string) =>
    `${person} leest dit, en kan de declaratie corrigeren en opnieuw indienen.`,
  financeRejectPlaceholder: "Waarom ze terugkomt…",

  // Het scherm van wie goedkeurt.
  financePerson: "Persoon",
  financeCategory: "Categorie",
  financeUncategorised: "Niet ingedeeld",
  financeSubmittedAt: "Ingediend",
  financeApprovedAt: "Goedgekeurd",
  financeOfWhichVat: (amount: string) => `incl. ${amount} btw`,
  financeWaitingTitle: "Wacht op een beslissing",
  financeWaitingEmptyTitle: "Er wacht niets",
  financeWaitingEmptyBody:
    "Declaraties die uw collega's indienen verschijnen hier, de oudste aankoop eerst.",
  financeOwedTitle: "Terug te betalen",
  financeOwedNote:
    "Goedgekeurde declaraties die uw collega's uit eigen zak hebben betaald. Een declaratie die de bedrijfskaart betaalde is goedgekeurd en is niemand iets schuldig, dus die staat hier niet.",
  financeOwedEmptyTitle: "Niemand krijgt nog iets terug",
  financeOwedEmptyBody:
    "Zodra u een declaratie goedkeurt die iemand zelf heeft betaald, wacht ze hier tot het geld teruggaat.",

  // Het eerste wat een medewerker van de module ziet.
  financeExpensesEmptyTitle: "Geen declaraties in deze periode",
  financeExpensesEmptyBody:
    "Leg vast wat u voor het werk hebt uitgegeven — de datum, het totaal op de bon en wiens geld betaalde. Ze blijft van u tot u ze indient.",

  // ---- de bank, en de stapel die ze achterlaat ----------------------------
  financeTabBank: "Bank",
  financeTabReconcile: "Afletteren",
  financeBankLoadFailed: "De rekeningafschriften konden niet worden geladen.",

  // Een afschrift importeren.
  financeBankImportStatement: "Afschrift importeren",
  financeBankImportTitle: "Een rekeningafschrift importeren",
  financeBankImportSubtitle:
    "We lezen het bestand eerst en tonen u wat we ervan hebben gemaakt. Er wordt niets opgeslagen tot u het zegt.",
  financeBankFile: "Afschriftbestand",
  financeBankFileHint:
    "Een CAMT.053- of MT940-download van uw bank, of een CSV-export.",
  financeBankAccount: "Rekening",
  financeBankAccountHint:
    "Het IBAN waarvoor dit afschrift geldt. Een CAMT.053- of MT940-bestand zegt het zelf; een CSV niet.",
  financeBankCurrencyHint:
    "Voor een CSV die het niet zegt. Laat leeg voor de eigen valuta van uw werkruimte.",
  financeBankCheckFile: "Dit bestand controleren",
  financeBankCheckAgain: "Opnieuw controleren",
  financeBankImport: "Importeren",
  financeBankReadFailed: "Dat bestand kon niet worden gelezen.",
  financeBankImportFailed: "Er is niets geïmporteerd.",
  financeBankStale:
    "U hebt gewijzigd hoe het bestand wordt gelezen. Controleer het opnieuw om het resultaat te zien.",
  financeBankStaged: (staged: number, duplicates: number) =>
    duplicates === 0
      ? `${staged} transacties geïmporteerd.`
      : `${staged} transacties geïmporteerd; ${duplicates} stonden er al en zijn ongemoeid gelaten.`,

  // Wat de server van het bestand heeft gemaakt.
  financeBankFormat: "Gelezen als",
  financeBankSourceCamt: "CAMT.053",
  financeBankSourceMt940: "MT940",
  financeBankSourceCsv: "CSV",
  financeBankRows: "Transacties",
  financeBankRowsRead: (lines: number, rows: number) =>
    `${lines} van ${rows} regels`,
  financeBankSkipped: "Regels die geen transactie zijn",
  financeBankUnbooked: "Nog niet door de bank geboekt",
  financeBankPeriod: "Periode",
  financeBankEncoding: "Codering",
  financeBankSampleTitle: "De eerste transacties, zoals wij ze lezen",
  financeBankSampleTruncated:
    "Hier ziet u alleen de eerste transacties. Ze worden allemaal geïmporteerd.",
  financeBankRowsRefused: (count: number) =>
    count === 1
      ? "Eén regel kan niet worden gelezen, dus er is niets geïmporteerd."
      : `${count} regels kunnen niet worden gelezen, dus er is niets geïmporteerd.`,
  financeBankRowAt: (line: number) => `Regel ${line}:`,
  financeBankRowUnknown: "Een regel:",

  // Ons zeggen welke kolom wat is.
  financeBankMappingTitle: "Welke kolom is wat",
  financeBankMappingNote:
    "We hebben het geraden uit de kop van het bestand zelf. Corrigeer wat we verkeerd hebben, en controleer het bestand daarna opnieuw.",
  financeBankColumnNone: "Niet in dit bestand",
  financeBankColDate: "Boekdatum",
  financeBankColValueDate: "Valutadatum",
  financeBankColAmount: "Bedrag (één kolom met teken)",
  financeBankColDebit: "Geld eruit",
  financeBankColCredit: "Geld erin",
  financeBankColSign: "Welke kant het op gaat",
  financeBankColCurrency: "Valuta per regel",
  financeBankColCounterparty: "Wie er betaald is, of wie betaalde",
  financeBankColIban: "Hun rekening",
  financeBankColRemittance: "Wat er bij de betaling stond",
  financeBankColReference: "De eigen referentie van de bank",
  financeBankDates: "Datums gelezen als",
  financeBankDecimal: "Centen gescheiden door",
  financeBankConventionAuto: "Uit het bestand afleiden",
  financeBankConventionDmy: "Dag/maand/jaar",
  financeBankConventionMdy: "Maand/dag/jaar",
  financeBankConventionYmd: "Jaar-maand-dag",
  financeBankConventionComma: "Een komma",
  financeBankConventionDot: "Een punt",

  // Wat er geïmporteerd is.
  financeBankLines: "Transacties",
  financeBankClosingBalance: "Eindsaldo",
  financeBankImportedAt: "Geïmporteerd",
  financeBankEmptyTitle: "Nog geen afschriften",
  financeBankEmptyBody:
    "Importeer een maand van uw bank en elke transactie erin belandt op één stapel, klaar om afgeletterd te worden tegen de factuur die ze betaalde.",

  // Het afletterscherm.
  financeBankStatement: "Afschrift",
  financeBankAllStatements: "Alles wat nog niet afgeletterd is",
  financeBankToMatchTitle: (count: number) =>
    count === 1
      ? "1 transactie af te letteren"
      : `${count} transacties af te letteren`,
  financeBankAllMatchedTitle: "Niets meer af te letteren",
  financeBankAllMatchedBody:
    "Elke transactie in de geïmporteerde afschriften is aan een factuur toegewezen of opzijgezet. Importeer nog een maand om verder te gaan.",
  financeBankCapped:
    "Deze lijst is een eerste stapel, niet alles — werk ze af en herlaad om de rest te zien.",
  financeBankBookedOn: "Geboekt",
  financeBankCounterparty: "Wie",
  financeBankNoCounterparty: "Geen naam bij de betaling",
  financeBankRemittance: "Referentie",
  financeBankCertain: "Zeker",
  financeBankThisOne: "Deze",
  financeBankNoGuess:
    "We hebben geen idee wat dit is. Kies de factuur, of zet de transactie opzij.",
  financeBankNotOurs: "Niet van ons",
  financeBankPickInvoice: "Een factuur kiezen",
  financeBankStillOwed: "nog openstaand",
  financeBankStillOwedIs: (amount: string) => `${amount} nog openstaand`,
  financeBankMatchFailed: "Die transactie is niet toegewezen.",
  financeBankUnmatchFailed: "Die aflettering is niet teruggedraaid.",
  financeBankIgnoreFailed: "Die transactie is niet opzijgezet.",

  // Waarom wij denken dat een transactie een document heeft voldaan.
  financeBankWhyNumberQuoted: "ons factuurnummer staat bij de betaling",
  financeBankWhyRuleSaved: "deze betaler is eerder zo afgeletterd",
  financeBankWhyCustomerNamed: (percent: number) =>
    `de naam bij de betaling lijkt op die van de klant (${percent}%)`,
  financeBankWhyWholeAmount: "het bedrag is precies wat openstaat",
  financeBankWhyOnlyDocument:
    "het is de enige openstaande factuur voor dit bedrag",
  financeBankWhyBeforeDue: (days: number) =>
    days === 1
      ? "ze kwam de dag voor de vervaldag binnen"
      : `ze kwam ${days} dagen voor de vervaldag binnen`,
  financeBankWhyAfterDue: (days: number) =>
    days === 1
      ? "ze kwam de dag na de vervaldag binnen"
      : `ze kwam ${days} dagen na de vervaldag binnen`,
  financeBankWhyPartPayment: (amount: string) =>
    `het is een deel van de factuur — ${amount} zou openblijven`,

  // Een transactie opzijzetten.
  financeBankIgnoreTitle: "Niet van ons om te boeken",
  financeBankIgnoreBody:
    "Zeg waarom, zodat wie dit afschrift na u leest het niet opnieuw hoeft uit te zoeken. Bankkosten, een privéoverboeking, een dubbele.",
  financeBankIgnore: "Opzijzetten",
  financeBankIgnorePlaceholder: "Waarom ze niet van ons is…",

  // De factuur met de hand kiezen.
  financeBankPickTitle: "Welke factuur heeft deze betaling voldaan?",
  financeBankPickSubtitle: (amount: string) =>
    `Er kwam ${amount} binnen. Zeg wat het heeft betaald.`,
  financeBankFindInvoice: "Een factuur zoeken",
  financeBankFindInvoiceHint:
    "Op nummer, of op de referentie die uw klant eraan gaf.",
  financeBankNoOpenInvoices:
    "Geen enkele uitgegeven factuur wacht nog op geld.",
  financeBankNoNumber: "Geen nummer",
  financeBankOverdue: "Achterstallig",
  financeBankConfirmMatch: "Deze is ermee voldaan",

  // Wat al afgehandeld is.
  financeBankUnmatched: "Af te letteren",
  financeBankMatched: "Afgeletterd",
  financeBankIgnored: "Opzijgezet",
  financeBankSettledTitle: "Al afgeletterd",
  financeBankSettledNote:
    "Elk hiervan legde een betaling vast en verplaatste de boeken. Er een terugdraaien keert dat om met een eigen boeking.",
  financeBankUndoMatch: "Terugdraaien",
  financeBankSetAsideTitle: "Opzijgezet",
  financeBankSetAsideNote:
    "Transacties waarvan iemand besloot dat ze niet van ons zijn.",
  financeBankUndoIgnore: "Terug op de stapel",

  // ---- alo Financiën: het rekeningschema ----------------------------------
  financeTabAccounts: "Rekeningen",
  financeChartLoadFailed: "Het rekeningschema kon niet worden geladen.",
  financeChartSeeded:
    "We zijn voor u begonnen met een neutraal rekeningschema. Elke rekening hierin is van u om te hernoemen of te hernummeren — de nummering van uw boekhouder breekt niets, want het boeken volgt de taak van elke rekening en niet haar nummer.",
  financeChartEmptyTitle: "Nog geen rekeningen",
  financeChartEmptyBody:
    "Het rekeningschema is de lijst van plaatsen waar geld kan zijn: de bank, wat klanten u schuldig zijn, wat u verdient, wat u uitgeeft. Er kan niets geboekt worden zolang er geen is.",

  financeAccountAdd: "Rekening toevoegen",
  financeAccountEdit: "Bewerken",
  financeAccountDelete: "Verwijderen",
  financeAccountCode: "Nummer",
  financeAccountCodeHint:
    "Het nummer dat uw boekhouder gebruikt. Letters en cijfers, geen spaties.",
  financeAccountName: "Naam",
  financeAccountRole: "Taak",
  financeAccountRoleHint:
    "Waar deze rekening automatisch voor dient. Facturen, betalingen en declaraties vinden hun rekening via haar taak, nooit via haar nummer — hernummeren is dus veilig, en een taak weghalen laat die documenten niet meer boeken tot een andere rekening ze heeft.",
  financeAccountType: "Soort",
  financeAccountTypeHint:
    "Wat de rekening bevat. Het bepaalt in welk rapport ze verschijnt.",
  financeAccountTypeUnset: "Kies er een…",
  financeAccountActive: "In gebruik",
  financeAccountActiveHint:
    "Een uitgefaseerde rekening behoudt haar geschiedenis en haar saldo en wordt niet meer aangeboden op nieuwe documenten.",
  financeAccountInUse: "In gebruik",
  financeAccountRetired: "Uitgefaseerd",
  financeAccountShowRetired: "Uitgefaseerde tonen",
  financeAccountMovement: "Mutatie",
  financeAccountPostings: "Boekingen",
  financeAccountSystemNote:
    "Wij hebben deze rekening aangemaakt, dus ze kan niet worden verwijderd — de boekhouding loopt erdoorheen. Hernoem ze, hernummer ze, of faseer ze uit.",
  financeAccountNewTitle: "Rekening toevoegen",
  financeAccountNewBody: "Uw eigen regel in uw eigen schema.",
  financeAccountEditTitle: "De rekening bewerken",
  financeAccountEditBody: "Hernoemen en hernummeren is altijd veilig.",
  financeAccountSaveFailed: "De rekening is niet opgeslagen.",
  financeAccountDeleteFailed: "De rekening is niet verwijderd.",

  // De vijf soorten, twee keer: het korte woord voor een tabelkop, en de zin
  // waarop iemand die er een kiest eigenlijk antwoordt.
  financeAccountTypeAsset: "Wat we bezitten",
  financeAccountTypeLiability: "Wat we schuldig zijn",
  financeAccountTypeEquity: "Eigen vermogen",
  financeAccountTypeIncome: "Wat we verdienen",
  financeAccountTypeExpense: "Wat we uitgeven",
  financeAccountTypeAssetLong:
    "Iets wat we bezitten of tegoed hebben — een bankrekening, kas, vorderingen op klanten",
  financeAccountTypeLiabilityLong:
    "Iets wat we schuldig zijn — leveranciers, belasting, geld dat we het personeel schuldig zijn",
  financeAccountTypeEquityLong:
    "Het aandeel van de eigenaars, en de saldi waarmee de boeken openden",
  financeAccountTypeIncomeLong: "Iets wat we verdienen",
  financeAccountTypeExpenseLong: "Iets wat we uitgeven",

  // De taken waar een boekingsregel doorheen loopt, elk gezegd als waar ze
  // voor dient.
  financeRoleNone: "Geen bijzondere taak",
  financeRoleAr: "Wat klanten ons schuldig zijn",
  financeRoleAp: "Wat wij leveranciers schuldig zijn",
  financeRoleBank: "De bankrekening waar het geld doorheen gaat",
  financeRoleCash: "Kleine kas",
  financeRoleVatOutput: "Btw die we in rekening brachten en schuldig zijn",
  financeRoleVatInput: "Btw die we betaalden en kunnen terugvorderen",
  financeRoleRevenue: "Omzet",
  financeRoleExpenseDefault: "Kosten zonder eigen categorie",
  financeRoleEmployeePayable: "Declaraties die we het personeel schuldig zijn",
  financeRoleFxDiff: "Koersverschillen",
  financeRoleRounding: "Afrondingsverschillen",
  financeRoleOpeningBalance: "De saldi waarmee de boeken openden",
  financeRoleSuspense: "Geld dat we nog niet kunnen plaatsen",

  // ---- alo Financiën: de vier rapporten -----------------------------------
  financeTabReports: "Rapporten",
  financeReportPl: "Winst-en-verliesrekening",
  financeReportBalance: "Balans",
  financeReportAged: "Wie wat schuldig is",
  financeReportVat: "Btw-aangifte",
  financeReportFrom: "Van",
  financeReportTo: "Tot",
  financeReportOn: "Op",
  financeReportShow: "Tonen",
  financeReportToday: "Vandaag",
  financeReportThisYear: "Dit jaar",
  financeReportThisQuarter: "Dit kwartaal",
  financeReportLastQuarter: "Vorig kwartaal",
  financeReportLastYearEnd: "Einde vorig jaar",
  financeReportDownloadCsv: "CSV downloaden",
  financeReportDownloadFailed: "Het bestand kon niet worden gedownload.",
  financeReportLoadFailed: "Het rapport kon niet worden geladen.",
  financeReportBasis: (from: string, to: string) =>
    `Alles wat tussen ${from} en ${to} is geboekt, beide dagen inbegrepen.`,
  financeReportBasisOn: (on: string) =>
    `Alles wat tot en met ${on} is geboekt.`,
  financeReportEmptyTitle: "Nog niets geboekt",
  financeReportEmptyBody:
    "Uitgegeven facturen, betalingen en goedgekeurde declaraties boeken zichzelf. Zodra er een boekt, verschijnt ze hier.",
  financeReportAmount: "Bedrag",
  financeReportTotal: "Totaal",
  financeReportPrevious: (from: string, to: string) => `${from} – ${to}`,

  // De winst-en-verliesrekening.
  financeReportIncome: "Wat we hebben verdiend",
  financeReportIncomeTotal: "Totaal verdiend",
  financeReportExpense: "Wat we hebben uitgegeven",
  financeReportExpenseTotal: "Totaal uitgegeven",
  financeReportProfit: "Winst",
  financeReportLoss: "Verlies",

  // De balans.
  financeReportAssets: "Wat we bezitten",
  financeReportAssetsTotal: "Totaal bezit",
  financeReportLiabilities: "Wat we schuldig zijn",
  financeReportLiabilitiesTotal: "Totaal schuld",
  financeReportEquity: "Eigen vermogen",
  financeReportEquityTotal: "Totaal eigen vermogen",
  financeReportResultToDate:
    "Winst of verlies tot nu toe, nog niet naar het eigen vermogen gebracht",
  financeReportLiabilitiesEquityTotal:
    "Schulden, eigen vermogen en resultaat samen",
  financeReportDifference: "Verschil",
  financeReportUnbalanced: (amount: string) =>
    `Deze boeken kloppen niet: er is een verschil van ${amount} dat nergens vandaan komt. Dien niets in op basis van deze balans — stuur ze in plaats daarvan naar ons.`,

  // Wie wat schuldig is.
  financeReportSide: "Weergave",
  financeReportReceivable: "Wat men ons schuldig is",
  financeReportPayable: "Wat wij schuldig zijn",
  financeReportParty: "Wie",
  financeReportBandCurrent: "Nog niet vervallen",
  financeReportBand1To30: "1–30 dagen",
  financeReportBand31To60: "31–60 dagen",
  financeReportBand61To90: "61–90 dagen",
  financeReportBand90Plus: "Meer dan 90 dagen",
  financeReportOpenDocuments: (count: number) =>
    count === 1 ? "1 openstaand document" : `${count} openstaande documenten`,
  financeReportNothingOwedToUs: "Niemand is u iets schuldig",
  financeReportNothingWeOwe: "U bent niemand iets schuldig",
  financeReportAgedEmptyBody:
    "Elk uitgegeven document aan deze kant is volledig voldaan.",
  financeReportUnconverted: (count: number) =>
    count === 1
      ? "1 document staat in geen van deze kolommen: we hebben geen wisselkoers om het in uw eigen valuta uit te drukken."
      : `${count} documenten staan in geen van deze kolommen: we hebben geen wisselkoers om ze in uw eigen valuta uit te drukken.`,

  // De btw-aangifte.
  financeReportVatRate: "Tarief",
  financeReportVatBase: "Bedrag exclusief btw",
  financeReportVatTax: "Btw",
  financeReportVatOutput: "Btw die we in rekening brachten",
  financeReportVatOutputTotal: "Totaal in rekening gebracht",
  financeReportVatInput: "Btw die we betaalden",
  financeReportVatInputTotal: "Totaal betaald",
  financeReportVatUnrated: "Zonder vermeld tarief",
  financeReportVatPayable: "Te betalen",
  financeReportVatRefund: "Terug te vorderen",
  financeReportVatNote:
    "Dit zijn de cijfers van uw boeken — verkopen én aankopen — en daaruit wordt een aangifte ingevuld. Het btw-overzicht onder Facturatie toont wat u hebt gefactureerd, en dat is een andere vraag.",

  // ---- de Financiën-agent: het voorstellen van categorieën ----------------
  agentActCategorise: "Categorieën voorstellen",
  agentCategoriseNote:
    "Kijkt naar uw eigen declaraties zonder categorie en stelt er voor elk een voor, uit de categorieën die u eerder voor die leverancier gebruikte. Er wordt niets ingedeeld tot u het aanvaardt.",
  agentCategoriseFieldPeriod: "Declaraties vanaf",
  agentCategoriseSuggested: (count: number): string =>
    count === 1 ? "1 voorstel" : `${count} voorstellen`,
  agentCategoriseNone: "Niets voor te stellen",
  agentCategoriseConsidered: (count: number): string =>
    count === 1 ? "1 declaratie bekeken" : `${count} declaraties bekeken`,
  agentCategoriseEvidence: (times: number): string =>
    times === 1
      ? "hier eerder één keer geboekt"
      : `hier eerder ${times} keer geboekt`,
  agentCategoriseAccept: "Aanvaarden",
  agentCategoriseDecline: "Nee",
  agentCategoriseAccepted: "Aanvaard",
  agentCategoriseDeclined: "Geweigerd",
  agentCategoriseLeftOut: "Overgeslagen",
  agentCategoriseNoMerchant: "Geen leverancier",
  agentCategoriseFooter:
    "Elk voorstel wacht op u — er wordt niets geboekt, aangegeven of teruggevorderd tot u het aanvaardt.",
  agentCategoriseFailed:
    "Dat kon niet worden beantwoord — probeer het opnieuw vanuit Financiën.",
  agentCategoriseReason: (reason: string): string => {
    switch (reason) {
      case "noMerchant":
        return "geen leverancier om ze aan te herkennen";
      case "noHistory":
        return "u hebt deze leverancier nog nooit ingedeeld";
      case "alreadyProposed":
        return "heeft al een voorstel";
      case "declined":
        return "u hebt hier een voorstel geweigerd";
      default:
        // Een reden die een nieuwere server kent en deze client niet: zeg dat
        // ze is overgeslagen in plaats van te doen alsof er iets voorgesteld is.
        return "overgeslagen";
    }
  },

  // ---- de Financiën-agent: de twee antwoorden -----------------------------
  agentActVatSummary: "Btw-cijfers",
  agentVatSummaryNote:
    "Leest de btw die uw boeken over die dagen dragen — btw in rekening gebracht, btw betaald, en het verschil. Er wordt niets aangegeven en niets gewijzigd.",
  agentVatFieldPeriod: "Periode",
  agentVatCharged: "In rekening gebracht op verkopen",
  agentVatPaid: "Betaald op aankopen",
  agentVatOwed: "U moet betalen",
  agentVatRefund: "U krijgt terug",
  agentVatBaseSales: "Omzet",
  agentVatBaseCosts: "Kosten",
  agentVatUnrated: "Zonder tarief",
  agentVatRateRow: (rate: string, base: string): string =>
    `${rate} van ${base}`,
  agentVatNothing: "Niets in deze dagen",
  agentVatFooter:
    "Cijfers voor een aangifte, geen aangifte — indienen gebeurt nog altijd in uw nationale portaal.",
  agentActFlagAnomalies: "De boeken nakijken",
  agentAnomalyNote:
    "Leest uw journaal over die dagen en noemt wat een tweede blik verdient, met de boekingen die erachter zitten. Het schrijft niets en markeert niets als nagekeken.",
  agentAnomalyFieldPeriod: "Boeken vanaf",
  agentAnomalyFound: (count: number): string =>
    count === 1 ? "1 om naar te kijken" : `${count} om naar te kijken`,
  agentAnomalyNone: "Niets viel op",
  agentAnomalyScanned: (count: number): string =>
    count === 1 ? "1 boeking gelezen" : `${count} boekingen gelezen`,
  agentAnomalyShown: (shown: number, found: number): string =>
    `${shown} van ${found} getoond`,
  agentAnomalyTruncated:
    "Deze dagen bevatten meer boekingen dan één controle leest — vraag het opnieuw voor een kortere periode om de rest te zien.",
  agentAnomalyNotComparable: (count: number): string =>
    count === 1
      ? "1 boeking noemt geen klant of leverancier, dus ze kon niet worden vergeleken"
      : `${count} boekingen noemen geen klant of leverancier, dus ze konden niet worden vergeleken`,
  agentAnomalyKind: (kind: string): string => {
    switch (kind) {
      case "duplicate":
        return "Twee keer geboekt in één week";
      case "unusualAmount":
        return "Anders dan de rest van deze rekening";
      case "missingRecurring":
        return "Een maand met niets erin";
      default:
        // Een soort die een nieuwere server kent en deze client niet: nog
        // altijd een vraag, nooit niets.
        return "Een blik waard";
    }
  },
  agentAnomalyTypical: (amount: string): string => `meestal ${amount}`,
  agentAnomalyMissingMonth: (month: string): string => `niets in ${month}`,
  agentAnomalyEvidence: "De boekingen die erachter zitten",
  agentAnomalyFooter:
    "Er is niets gewijzigd en niets als nagekeken gemarkeerd — elk hiervan is een vraag over boekingen, en het antwoord op één is een corrigerende boeking.",

  // ---- alo Voorraad (B5.09a–c, B5.10; vertaald bij B5.11) -------------------
  //
  // De woorden zijn die van een magazijn, niet van een grootboek: "op
  // voorraad", "wij betalen". Twee keuzes gelden overal in dit blok. Goederen
  // worden *ingeslagen* en *uitgeslagen* — de eigen werkwoorden van een
  // magazijn, niet "gepickt"; en niets hier noemt een aantal, een waarde of een
  // regel die van de server is: een weigering wordt in de zin van de server
  // zelf getoond.
  moduleInventory: "Voorraad",
  inventoryTabCatalog: "Catalogus",
  inventoryTabStock: "Voorraadstand",
  inventoryLoadFailed: "Uw catalogus kon niet worden geladen.",
  inventorySaveFailed: "De wijziging kon niet worden opgeslagen.",
  inventoryHistoryFailed: "Die geschiedenis kon niet worden geladen.",
  inventoryClose: "Sluiten",
  inventoryEdit: "Bewerken",
  inventoryArchive: "Archiveren",
  inventoryRestore: "Terugzetten",
  inventoryArchived: "gearchiveerd",
  inventoryColActions: "Acties",
  inventoryNoMatches: "Niets hier komt overeen met wat u hebt getypt.",

  // De catalogus: de prijslijst, gezien als dingen.
  inventoryNewProduct: "Nieuw product",
  inventorySearchCatalog: "Zoek op naam, code of streepjescode",
  inventoryStockedOnly: "Alleen voorraadartikelen",
  inventoryShowArchived: "Gearchiveerde tonen",
  inventoryCatalogEmptyTitle: "Uw catalogus is leeg",
  inventoryCatalogEmptyBody:
    "Een product is hier één record: wat u ervoor rekent, wat u ervoor betaalt en — als het iets is dat u op een schap bewaart — hoeveel u ervan hebt. Voeg het eerste toe en het kan diezelfde dag op een factuur en in een magazijn staan.",
  inventoryColProduct: "Product",
  inventoryColSku: "Code",
  inventoryColBarcode: "Streepjescode",
  inventoryColOnHand: "Op voorraad",
  inventoryColPurchasePrice: "Wij betalen",
  inventoryColSalePrice: "Wij rekenen",
  inventoryColVatRate: "Btw",
  inventoryTypeStocked: "Voorraadartikel",
  inventoryTypeService: "Dienst",
  inventoryNotStocked: "—",
  inventoryArchiveProductConfirm: (name: string) =>
    `${name} archiveren? Het blijft op elk document staan dat er al mee is opgemaakt en wordt niet meer aangeboden op nieuwe. U kunt het altijd terugzetten.`,

  // De velden van de productfiche, die de prijslijst van Facturatie en deze
  // catalogus delen. De twee aanwijzingen die ertoe doen gaan over een regel
  // van de server: het controlecijfer van een streepjescode, en wat
  // "voorraadartikel" beslist.
  inventoryFieldSku: "Code (SKU)",
  inventorySkuHint:
    "Uw eigen code voor dit artikel. Uniek binnen uw producten; laat het leeg als u er geen hebt.",
  inventoryFieldBarcode: "Streepjescode",
  inventoryBarcodeHint:
    "De GTIN op de doos. Het controlecijfer wordt nagerekend, dus een verkeerd getypte code wordt hier geweigerd in plaats van ontdekt wanneer het verkeerde artikel vertrekt.",
  inventoryFieldPurchasePrice: "Inkoopprijs",
  inventoryPurchasePriceHint: "Wat u ervoor betaalt, in uw eigen valuta.",
  inventoryFieldDefaultSupplier: "Vaste leverancier",
  inventoryDefaultSupplierHint:
    "Bij wie dit normaal wordt gekocht. Een bestelvoorstel vertrekt hiervan.",
  inventoryNoSupplier: "Niemand in het bijzonder",
  inventoryFieldStocked: "Voorraad",
  inventoryStockedLabel: "Hiervan een aantal bijhouden",
  inventoryStockedHint:
    "Alleen een voorraadartikel kan zich tussen plaatsen verplaatsen. Een dienst kan niet worden ontvangen, geleverd of geteld — en zodra er iets is verplaatst, kan dit niet meer worden uitgezet.",

  // De voorraadlijst, en wat haar cijfers betekenen.
  inventorySearchStock: "Zoek op product, code of plaats",
  inventoryFilterLocation: "Plaats",
  inventoryAllLocations: "Overal",
  inventoryShowCounterparties: "Tegenpartijen tonen",
  inventoryCounterpartiesNote:
    "Leveranciers, klanten, aanpassingen en productie zijn tegenpartijen, geen plaatsen: zij zijn het andere eind van elke beweging. Met hen erbij telt het totaal hieronder op tot ongeveer niets — zo ziet een grootboek eruit dat sluit, niet een leeg magazijn.",
  inventoryStockEmptyTitle: "Er ligt nog niets op het schap",
  inventoryStockEmptyBody:
    "Voorraad verschijnt hier zodra er iets beweegt: een inkooporder die u ontvangt, een levering die u verstuurt, of een aanpassing die u met de hand maakt. Er valt geen aantal te typen — wat hier staat is de som van alles wat er is gebeurd.",
  inventoryColLocation: "Plaats",
  inventoryColValue: "Waarde",
  inventoryColLastMove: "Laatste beweging",
  inventoryOpenHistory: "Geschiedenis",
  inventoryReferenceValue: (total: string) =>
    `${total} tegen de inkoopprijzen van vandaag — een richtbedrag voor wat hier staat, geen boekhoudkundig saldo.`,

  // De bewegingsgeschiedenis: van → naar, hoeveel, waarom, welk document.
  inventoryHistoryTitle: (product: string) => `${product} — bewegingen`,
  inventoryHistorySubtitle: (place: string) =>
    `Alles wat ${place} in of uit ging.`,
  inventoryHistoryEmpty: "Er is nog niets deze plaats in of uit gegaan.",
  inventoryHistoryCapped: (limit: number) =>
    `De ${limit} recentste bewegingen worden getoond. Oudere blijven vastgelegd.`,
  inventoryColWhen: "Wanneer",
  inventoryColMovement: "Van → naar",
  inventoryColQuantity: "Aantal",
  inventoryColWhy: "Reden",
  inventoryColDocument: "Document",
  inventoryNoDocument: "Met de hand",

  // Wat een plaats is. De vier tegenpartijen heten wat ze voor een magazijn
  // betekenen, niet wat de draad ze noemt.
  inventoryKindStock: "Magazijn",
  inventoryKindTransit: "Onderweg",
  inventoryKindSupplier: "Leverancier",
  inventoryKindCustomer: "Klant",
  inventoryKindAdjust: "Aanpassing",
  inventoryKindProduction: "Productie",

  // Waarom er iets bewoog.
  inventoryReasonReceipt: "Ontvangst",
  inventoryReasonDelivery: "Levering",
  inventoryReasonTransfer: "Verplaatsing",
  inventoryReasonAdjustment: "Aanpassing",
  inventoryReasonReturn: "Retour",
  inventoryReasonShrinkage: "Derving",
  inventoryReasonCount: "Telling",

  // De reden die iemand opgaf voor een aanpassing met de hand.
  inventoryAdjustDamaged: "Breuk",
  inventoryAdjustLost: "Verlies",
  inventoryAdjustFound: "Overschot",
  inventoryAdjustExpired: "Verlopen",
  inventoryAdjustTheft: "Diefstal",
  inventoryAdjustSample: "Monster",
  inventoryAdjustCorrection: "Correctie",

  // ---- de twee orderdocumenten (B5.09b) -------------------------------------
  //
  // Een zin die aan een onomkeerbare daad voorafgaat zegt wat ze zal doen, niet
  // "weet u het zeker". Een order plaatsen trekt een nummer uit een reeks
  // zonder gaten en schrijft een brief; een aankomst boeken verplaatst echte
  // goederen en maakt een inkoopfactuur op. Geen van beide kan ongedaan.
  inventoryTabPurchasing: "Inkoop",
  inventoryTabSales: "Verkooporders",
  inventoryOrdersLoadFailed: "Die orders konden niet worden geladen.",
  inventoryOrderLoadFailed: "Die order kon niet worden geladen.",
  inventoryDraftOrder: "Concept",
  inventoryDraftInvoice: "Conceptfactuur",
  inventoryOrderLate: "Te laat",
  inventoryFilterStatus: "Status",
  inventoryAllStatuses: "Elke status",
  inventoryNoOrdersInState: "Geen orders met die status",
  inventoryCancelAction: "Annuleren",

  // Hoe een status heet. "Geannuleerd" is gedeeld: een opgegeven order is
  // opgegeven, welke kant de goederen ook op gingen.
  inventoryOrderStatusCancelled: "Geannuleerd",
  inventoryPoStatusDraft: "Concept",
  inventoryPoStatusSent: "Geplaatst",
  inventoryPoStatusPartial: "Deels ontvangen",
  inventoryPoStatusReceived: "Ontvangen",
  inventorySoStatusDraft: "Concept",
  inventorySoStatusConfirmed: "Bevestigd",
  inventorySoStatusPartial: "Deels geleverd",
  inventorySoStatusDelivered: "Geleverd",

  // De twee lijsten.
  inventorySearchPurchaseOrders: "Zoek op nummer, leverancier of referentie",
  inventorySearchSalesOrders: "Zoek op nummer, klant of referentie",
  inventoryNewPurchaseOrder: "Nieuwe inkooporder",
  inventoryNewSalesOrder: "Nieuwe verkooporder",
  inventoryPurchaseOrdersEmptyTitle: "U hebt nog niets besteld",
  inventoryPurchaseOrdersEmptyBody:
    "Een inkooporder legt vast wat u een leverancier hebt gevraagd. Maak er een als concept, plaats ze wanneer u zover bent, en boek wat binnenkomt ertegen af — het voorraadgrootboek wordt voor u geschreven.",
  inventorySalesOrdersEmptyTitle: "Nog geen klant heeft iets besteld",
  inventorySalesOrdersEmptyBody:
    "Een verkooporder legt vast wat een klant u heeft gevraagd. Maak er een als concept, bevestig ze om ze een nummer te geven, en boek elke zending af zodra ze buitengaat — de factuur brengt in rekening wat er werkelijk is gegaan.",
  inventoryColOrder: "Order",
  inventoryColSupplier: "Leverancier",
  inventoryColCustomer: "Klant",
  inventoryColExpected: "Verwacht op",
  inventoryColPromised: "Beloofd op",
  inventoryColState: "Status",
  inventoryColTotal: "Totaal",

  // Het document.
  inventoryBackToPurchaseOrders: "Alle inkooporders",
  inventoryBackToSalesOrders: "Alle verkooporders",
  inventoryCreateDraft: "Concept aanmaken",
  inventorySaveDraft: "Opslaan",
  inventoryPrintOrder: "Afdrukken",
  inventoryUnsavedNotice:
    "Deze wijzigingen zijn nog niet opgeslagen, dus de totalen hieronder zijn de laatste die de server heeft uitgerekend.",
  inventoryOrderFrozenNotice:
    "Deze order is geplaatst. Ze draagt een nummer dat de leverancier in handen heeft, dus ze kan niet meer worden bewerkt — boek af wat er binnenkomt, of annuleer ze.",
  inventorySalesOrderFrozenNotice:
    "Deze order is bevestigd. Ze draagt een nummer dat de klant in handen heeft, dus ze kan niet meer worden bewerkt — boek elke zending af zodra ze buitengaat.",
  inventoryFixLinesFirst:
    "Een van de regels is niet af. Corrigeer die en sla opnieuw op.",
  inventoryOrderNeedsSupplier:
    "Kies de leverancier bij wie deze order wordt geplaatst.",
  inventoryOrderNeedsCustomer: "Kies de klant voor wie deze order is.",
  inventoryPickSupplier: "Kies een leverancier",
  inventoryPickCustomer: "Kies een klant",
  inventorySupplierHint:
    "Bij wie u bestelt. Dit kan niet meer worden gewijzigd zodra de order is geplaatst.",
  inventoryCustomerHint:
    "Voor wie de order is. Dit kan niet meer worden gewijzigd zodra de order is bevestigd.",
  inventoryExpectedHint:
    "De dag waarop u de goederen verwacht. Een order die daaroverheen gaat, wordt als te laat gemarkeerd.",
  inventoryPromisedHint:
    "De dag waarop u de goederen hebt beloofd. Een order die daaroverheen gaat, wordt als te laat gemarkeerd.",
  inventoryFieldReference: "Referentie",
  inventoryReferenceHint:
    "Uw eigen referentie voor deze order — een project, een werf, een dossiernummer.",
  inventoryFieldOrdered: "Geplaatst op",
  inventoryFieldConfirmed: "Bevestigd op",
  inventoryFieldNote: "Notitie",
  inventoryOrderNoteHint:
    "Alles wat de andere partij moet lezen. Het wordt op de order afgedrukt.",

  // Het regelraster. De woorden zijn die van een document, want deze regels
  // worden er een.
  inventoryLines: "Regels",
  inventoryAddLine: "Regel toevoegen",
  inventoryNoLines: "Nog geen regels.",
  inventoryColDescription: "Omschrijving",
  inventoryColUnit: "Eenheid",
  inventoryColUnitPrice: "Stukprijs",
  inventoryColNet: "Netto",
  inventoryColReceived: "Ontvangen",
  inventoryColDelivered: "Geleverd",
  inventoryColOutstanding: "Openstaand",
  inventoryColToBill: "Te factureren",
  inventoryPickProduct: "Uit de catalogus",
  inventoryDescriptionPlaceholder: "Wat er wordt besteld",
  inventoryUnitPlaceholder: "stuk",
  inventoryQtyPlaceholder: "1",
  inventoryAmountPlaceholder: "0,00",
  inventoryRatePlaceholder: "0",
  inventoryRemoveLine: "Regel verwijderen",
  inventoryLineNeedsDescription: "Zeg waarvoor deze regel is.",
  inventoryNotAQuantity: "Dat is geen aantal.",
  inventoryNotAnAmount: "Dat is geen bedrag.",
  inventoryNotARate: "Dat is geen percentage.",

  // De order plaatsen: één daad, en de zin noemt alle drie de delen ervan.
  inventorySendOrder: "Order plaatsen",
  inventorySendOrderConfirm:
    "Dit geeft de order haar nummer, bevriest ze voorgoed, en zet de begeleidende brief met de afgedrukte order als bijlage in uw Concepten. Er wordt niets verstuurd tot u het zelf verstuurt.",
  inventoryOrderPlacedNotice: (to: string, file: string) =>
    `De order is geplaatst. Een begeleidende brief aan ${to} met ${file} als bijlage wacht in uw Concepten — er is niets verstuurd.`,
  inventoryConfirmOrder: "Order bevestigen",
  inventoryConfirmOrderConfirm:
    "Dit geeft de order haar nummer en bevriest ze voorgoed. Er wordt geen bericht geschreven: de klant inlichten is een gewone brief die u zelf verstuurt.",
  inventoryCancelOrder: "Order annuleren",
  inventoryCancelOrderConfirm:
    "De order blijft bewaard en leesbaar, maar er wordt niets meer tegen verwacht.",
  inventoryCancelShortConfirm:
    "Een deel van deze order is al bewogen. Ze annuleren betekent dat wat tot nu toe is afgehandeld als het geheel wordt aanvaard, en dat er niets meer wordt verwacht. De order blijft leesbaar.",
  inventoryDiscardDraft: "Concept weggooien",
  inventoryDiscardDraftConfirm:
    "Dit concept heeft geen nummer en is aan niemand getoond, dus het wordt verwijderd in plaats van geannuleerd.",

  // Een zending boeken, in beide richtingen.
  inventoryReceiveGoods: "Aankomst boeken",
  inventoryDeliverGoods: "Zending boeken",
  inventoryReceiveTitle: (order: string) =>
    `Wat er is binnengekomen op ${order}`,
  inventoryDeliverTitle: (order: string) => `Wat er buitengaat op ${order}`,
  inventoryReceiveSubtitle:
    "Elke regel opent op wat nog openstaat. Wijzig wat u tekortkomt; de rest blijft in bestelling. Voor wat is binnengekomen wordt een conceptinkoopfactuur opgemaakt.",
  inventoryDeliverSubtitle:
    "Elke regel opent op wat nog openstaat. Wijzig wat er nu buitengaat; de rest blijft op de order staan.",
  inventoryReceiveWhere: "Ingeslagen op",
  inventoryReceiveWhereHint:
    "Waar de goederen werkelijk zijn weggezet. Het voorraadgrootboek wordt op deze plaats geschreven.",
  inventoryDeliverWhere: "Uitgeslagen van",
  inventoryDeliverWhereHint:
    "Waar de goederen zijn weggehaald. Het voorraadgrootboek wordt op deze plaats geschreven.",
  inventoryColThisConsignment: "Deze keer",
  inventoryFulfilNoteHint:
    "Wat degene die het afhandelde erbij schreef — een beschadigde krat, een deelzending.",
  inventoryFulfilNeedsPlace: "Kies eerst de plaats.",
  inventoryFulfilNeedsSomething:
    "Op geen enkele regel staat iets, dus er valt niets te boeken.",
  inventoryNoPlaces: "Nog geen plaatsen",
  inventoryBookArrival: "Inboeken",
  inventoryBookConsignment: "Uitboeken",
  inventoryArrivalBooked:
    "De aankomst is geboekt, het voorraadgrootboek is geschreven, en een conceptinkoopfactuur wacht op goedkeuring.",
  inventoryConsignmentBooked:
    "De zending is geboekt en het voorraadgrootboek is geschreven.",

  // Wat er al bewoog, en wat ervoor is gefactureerd.
  inventoryArrivals: "Aankomsten",
  inventoryNoArrivals: "Er is nog niets binnengekomen op deze order.",
  inventoryArrivalNo: (n: number) => `Aankomst ${n}`,
  inventoryBillDrafted: "Inkoopfactuur in concept",
  inventoryConsignments: "Zendingen",
  inventoryNoConsignments: "Er is nog niets buitengegaan op deze order.",
  inventoryConsignmentNo: (n: number) => `Zending ${n}`,
  inventoryRaiseInvoice: "Factureren wat is gegaan",
  inventoryRaisedInvoices: "Facturen",
  inventoryNoRaisedInvoices: "Er is nog niets gefactureerd vanuit deze order.",
  inventoryInvoiceDrafted:
    "Er is een conceptfactuur opgemaakt voor wat er is buitengegaan. Ze draagt geen nummer tot iemand haar in Facturatie uitgeeft.",

  // ---- streepjescodes lezen (B5.09c) ----------------------------------------
  //
  // De woorden volgen de hardware: een handscanner is een toetsenbord, dus het
  // veld is de hoofdzaak en de camera is slechts een tweede manier.
  inventoryScan: "Scannen",
  inventoryScanTitle: "Een streepjescode scannen",
  inventoryScanSubtitle:
    "Scan in het veld met een handscanner, of typ de code. Op een telefoon kunt u in plaats daarvan de camera gebruiken.",
  inventoryScanFieldCode: "Streepjescode",
  inventoryScanPlaceholder: "4006381333931",
  inventoryScanHint:
    "Een handscanner typt de code hier en drukt Enter voor u. Spaties en koppeltekens worden genegeerd.",
  inventoryScanLookup: "Zoek hem op",
  inventoryScanFailed: "Die code kon niet worden opgezocht.",
  inventoryScanWaiting: "Wacht op een code.",
  inventoryScanCameraStart: "Camera gebruiken",
  inventoryScanCameraStop: "Camera stoppen",
  inventoryScanCameraFailed:
    "De camera kon niet worden gestart. Geef er toegang toe, of typ de code — een handscanner heeft helemaal geen toestemming nodig.",
  inventoryScanAiming:
    "Richt de camera op de streepjescode. Ze stopt zodra er een is gelezen.",
  inventoryScanNoCamera:
    "Deze browser kan geen streepjescode van een camera lezen. Een handscanner werkt hier wel: die typt in het veld hierboven.",
  inventoryScanOnHand: (quantity: string) =>
    `${quantity} op voorraad, over alle plaatsen samen.`,
  inventoryScanNowhere: "Er ligt er nog nergens een.",
  inventoryScanServiceNote:
    "Dit is een dienst, dus er is geen aantal van te vinden.",
  inventoryScanOpenProduct: "Dit product openen",
  inventoryScanShowInStock: "In de lijst tonen",
  inventoryScanAddProduct: "Met deze streepjescode aan de catalogus toevoegen",

  // De voorraadagent (ADR 0035, B5.10). Elk woord houdt een concept een
  // concept: de kaart mag een lezer nooit laten geloven dat een leverancier is
  // benaderd.
  agentActReorderProposals: "Bestellingen voorbereiden",
  agentReorderNote:
    "Kijkt naar alles waarvan u onder uw eigen minimum zit en schrijft één conceptinkooporder per leverancier. Er wordt niets verstuurd — elk concept wacht bij uw inkooporders tot u het nakijkt en verstuurt.",
  agentActStockAnswer: "Voorraad nakijken",
  agentStockAnswerNote:
    "Leest hoe één product er nu voor staat: op uw schappen, in bestelling, aan klanten beloofd. Wijzigt niets en reserveert niets.",
  agentFieldSupplier: "Leverancier",
  agentFieldLocation: "Plaats",
  agentFieldProduct: "Product",
  agentReorderEverySupplier: "Elke leverancier",
  agentReorderEverywhere: "Overal",
  agentReorderShortages: (count: number): string =>
    count === 1 ? "1 onder minimum" : `${count} onder minimum`,
  agentReorderNothingShort: "Niets zit onder zijn minimum",
  agentReorderDrafted: (count: number): string =>
    count === 1 ? "1 conceptorder" : `${count} conceptorders`,
  agentReorderLines: (count: number): string =>
    count === 1 ? "1 regel" : `${count} regels`,
  agentReorderLeftOut: "Niets besteld voor",
  agentReorderReason: (reason: string): string => {
    switch (reason) {
      case "noSupplier":
        return "niemand heeft er u een prijs voor gegeven";
      case "nothingToBuy":
        return "de regel vraagt om niets";
      default:
        // Een reden die een nieuwere server kent en deze client niet: nog
        // altijd zichtbaar weggelaten, nooit stilzwijgend geschrapt.
        return "weggelaten";
    }
  },
  agentReorderNeeded: (qty: string, unit: string): string =>
    unit === "" ? `${qty} te bestellen` : `${qty} ${unit} te bestellen`,
  agentReorderFooter:
    "Dit zijn concepten. Er is geen leverancier benaderd en er is geen ordernummer getrokken — open er een in Voorraad om ze na te kijken en te versturen.",
  agentStockOnHand: "Op de schappen",
  agentStockOnOrder: "In bestelling",
  agentStockCommitted: "Aan klanten beloofd",
  agentStockAvailable: "Dat laat over",
  agentStockNoShelf: "Een dienst — hiervan wordt niets bijgehouden",
  agentStockNowhere: "Nergens",
  agentStockWatched: "Voorraadregel",
  agentStockMinimum: (min: string, target: string): string =>
    `minimum ${min}, aanvullen tot ${target}`,
  agentStockBelowMinimum: "onder minimum",
  agentStockFooter:
    "Cijfers zoals ze er nu voor staan. Er is niets besteld en er is niets apart gezet.",
  sitesTranslateWholeSite: "Hele website vertalen",
  sitesWholeTranslationPreparing:
    "Volledige vertaling wordt voorbereid ter controle…",
  sitesWholeTranslationPrepareFailed:
    "De vertaling kon niet worden voorbereid. Er is niets gewijzigd; vertaal pagina’s handmatig of probeer opnieuw.",
  sitesWholeTranslationApplyFailed:
    "De vertaling kon niet worden toegepast. Er is niets gewijzigd; maak een nieuwe controle en probeer opnieuw.",
  sitesWholeTranslationReview: (language: string) =>
    `Controleer de ${language}-vertaling`,
  sitesWholeTranslationReviewHint:
    "Vergelijk elke pagina en elk bericht. Er wordt niets opgeslagen voordat u dit goedkeurt.",
  sitesWholeTranslationApprove: "Vertaling goedkeuren",
  sitesTranslationPageKind: "Pagina",
  sitesTranslationPostKind: "Bericht",
  sitesCatalogs: "Aanbod",
  sitesCatalogsHint:
    "Wat deze website aanbiedt — gerechten, kamers, diensten, opleidingen. Prijzen liggen vast zodra u publiceert.",
  sitesCatalogsLoading: "Aanbod laden...",
  sitesCatalogsLoadFailed:
    "Het aanbod kon niet worden geladen. Controleer uw verbinding en probeer het opnieuw.",
  sitesCatalogLoadFailed:
    "Deze lijst kon niet worden geopend. Controleer uw verbinding en probeer het opnieuw.",
  sitesNewCatalog: "Nieuwe lijst",
  sitesCatalogNoneTitle: "Nog niets in het aanbod",
  sitesCatalogNoneBody:
    "Een lijst is wat uw website toont — en, als u dat wilt, waaruit besteld wordt. Begin met één naam en één munt; de artikelen volgen daarna.",
  sitesCatalogOrdersOn: "Neemt bestellingen aan",
  sitesCatalogOrdersOff: "Geen bestelformulier",
  sitesCatalogSettings: "Deze lijst",
  sitesCatalogSettingsHint:
    "De naam is alleen voor u; bezoekers zien de artikelen. Wijzigingen bereiken de live website bij uw volgende publicatie.",
  sitesCatalogName: "Naam van de lijst",
  sitesCatalogCurrency: "Munt",
  sitesCatalogCurrencyHint:
    "Drie letters, bijvoorbeeld EUR. Wijzigen leest de al ingevoerde prijzen in de nieuwe munt — het rekent ze niet om.",
  sitesCatalogOrders: "Bestellingen aannemen uit deze lijst",
  sitesCatalogOrdersHint:
    "Bezoekers krijgen een bestelformulier onder de lijst. Er wordt niets betaald op de website: de bestelling komt in uw postvak en u bevestigt ze zelf. Ze verschijnt bij uw volgende publicatie.",
  sitesCatalogCreate: "Lijst aanmaken",
  sitesCatalogSave: "Lijst opslaan",
  sitesCatalogSaveFailed: "De lijst kon niet worden opgeslagen.",
  sitesCatalogDelete: "Lijst verwijderen",
  sitesCatalogDeleteConfirm: "Verwijderen, met alles erin",
  sitesCatalogDeleteHint:
    "De artikelen en groepen gaan mee. Al gepubliceerde pagina’s tonen wat ze toonden tot u opnieuw publiceert.",
  sitesCatalogDeleteFailed: "De lijst kon niet worden verwijderd.",
  sitesCatalogGroups: "Groepen",
  sitesCatalogGroupsHint:
    "Optioneel. Een groep is één tussenkop op de pagina — Broden, Kamers, Opleidingen van een halve dag.",
  sitesCatalogGroupName: "Naam van de groep",
  sitesCatalogNewGroup: "Nieuwe groep",
  sitesCatalogNewGroupPlaceholder: "Broden",
  sitesCatalogAddGroup: "Groep toevoegen",
  sitesCatalogGroupRemove: (name: string) => `Groep ${name} verwijderen`,
  sitesCatalogGroupRemoveShort: "Verwijderen",
  sitesCatalogGroupSaveFailed: "De groep kon niet worden opgeslagen.",
  sitesCatalogGroupDeleteFailed: "De groep kon niet worden verwijderd.",
  sitesCatalogItems: "Artikelen",
  sitesCatalogItemsHint:
    "Alles wat deze lijst aanbiedt, in de volgorde waarin de pagina het toont.",
  sitesCatalogAddItem: "Artikel toevoegen",
  sitesCatalogNoItemsTitle: "Deze lijst is leeg",
  sitesCatalogNoItemsBody:
    "Voeg toe wat u aanbiedt. Een naam volstaat om te beginnen — prijs, foto en beschrijving kunnen volgen.",
  sitesCatalogNoPrice: "Prijs op aanvraag",
  sitesCatalogEdit: "Bewerken",
  sitesCatalogEditItem: (name: string) => `${name} bewerken`,
  sitesCatalogNewItem: "Nieuw artikel",
  sitesCatalogSaveItem: "Artikel opslaan",
  sitesCatalogItemSubtitle:
    "Het verschijnt op de website bij uw volgende publicatie.",
  sitesCatalogItemName: "Naam",
  sitesCatalogItemHandle: "Kenmerk",
  sitesCatalogItemHandlePlaceholder: "Uit de naam",
  sitesCatalogItemHandleHint:
    "De korte naam die in links en op bestellingen wordt gebruikt. Laat het leeg en wij maken er een uit de naam.",
  sitesCatalogItemPrice: (currency: string) => `Prijs (${currency})`,
  sitesCatalogItemPriceHint:
    "Schrijf hem zoals op een kaart — 4.50 of 4,50. Laat leeg voor prijs op aanvraag.",
  sitesCatalogItemPriceNote: "Naast de prijs",
  sitesCatalogItemPriceNoteHint:
    "Een korte toevoeging — per nacht, vanaf, per persoon.",
  sitesCatalogItemGroup: "Groep",
  sitesCatalogItemNoGroup: "Geen groep",
  sitesCatalogItemDescription: "Beschrijving",
  sitesCatalogItemPhoto: "Foto",
  sitesCatalogItemPhotoNone: "Nog geen foto",
  sitesCatalogItemPhotoNoneHint:
    "Een item zonder foto verschijnt gewoon, met zijn naam, prijs en beschrijving.",
  sitesCatalogItemPhotoAdd: "Foto toevoegen",
  sitesCatalogItemPhotoReplace: "Vervangen",
  sitesCatalogItemPhotoRemove: "Foto verwijderen",
  sitesCatalogItemPhotoPreview: "De foto van dit item",
  sitesCatalogItemPhotoAlt: "Wat de foto toont",
  sitesCatalogItemPhotoAltHint:
    "Wordt voorgelezen door schermlezers. Beschrijf de foto — niet de naam die eronder staat.",
  sitesCatalogItemPhotoAltMissing:
    "Niemand heeft deze foto nog beschreven; tot dan valt de kaart terug op de naam van het item.",
  sitesCatalogItemAvailability: "Beschikbaarheid",
  sitesCatalogAvailabilityHint:
    "Uitverkocht blijft zichtbaar, gemarkeerd en niet bestelbaar. Verborgen wordt helemaal niet gepubliceerd.",
  sitesCatalogAvailable: "Beschikbaar",
  sitesCatalogSoldOut: "Uitverkocht",
  sitesCatalogHidden: "Verborgen",
  sitesCatalogItemSaveFailed: "Het artikel kon niet worden opgeslagen.",
  sitesCatalogItemDelete: "Verwijderen",
  sitesCatalogItemDeleteConfirm: "Definitief verwijderen",
  sitesCatalogItemDeleteLabel: (name: string) => `Verwijderen: ${name}`,
  sitesCatalogItemDeleteConfirmLabel: (name: string) =>
    `Definitief verwijderen: ${name}`,
  sitesCatalogItemDeleteFailed: "Het artikel kon niet worden verwijderd.",
  sitesSectionCatalog: "Catalogus",
  sitesSectionCatalogDesc:
    "Wat u aanbiedt, met prijzen, uit uw eigen catalogus.",
  sitesCatalogSectionHeading: "Kop erboven",
  sitesCatalogSectionChoose: "Welke catalogus",
  sitesCatalogSectionGroup: "Welke groep",
  sitesCatalogSectionAllGroups: "Alles uit de catalogus",
  sitesCatalogSectionGroupHint:
    "Toon één groep op deze pagina — de lunchkaart, de tweepersoonskamers — of alles.",
  sitesCatalogSectionGoneGroup: (handle: string) =>
    `${handle} (bestaat niet meer als groep)`,
  sitesCatalogSectionOneGroup: (handle: string) => `Eén groep: ${handle}`,
  sitesCatalogSectionNoCatalogs: "Deze site heeft nog geen catalogus",
  sitesCatalogSectionNoCatalogsHint:
    "Een catalogus bevat wat u aanbiedt, met de prijzen. Maak er één en deze sectie kan die tonen.",
  sitesCatalogSectionOrdersOn:
    "Deze catalogus neemt bestellingen aan, dus de gepubliceerde pagina krijgt een bestelformulier onder de lijst. Bestellingen komen binnen in de bestellijst van deze site.",
  sitesCatalogSectionOrdersOff:
    "Deze catalogus neemt geen bestellingen aan, dus de pagina toont alleen de lijst. Bestellen zet u aan op de catalogus, niet op deze sectie.",
  // Wat een bezoeker kan reserveren, en de agenda waarin de afspraak terechtkomt
  // (S2.13c).
  sitesBookings: "Afspraken",
  sitesBookingsHint:
    "Wat een bezoeker op deze website kan reserveren — een gesprek, een bezichtiging, een tafel. Elke afspraak komt rechtstreeks in een van uw agenda’s.",
  sitesBookingsLoading: "Bezig met laden wat er te reserveren valt...",
  sitesBookingsLoadFailed:
    "De reserveerbare diensten konden niet worden geladen. Controleer uw verbinding en probeer het opnieuw.",
  sitesNewBooking: "Nieuwe reserveerbare dienst",
  sitesBookingNoneTitle: "Er valt nog niets te reserveren",
  sitesBookingNoneBody:
    "Een reserveerbare dienst is één ding waarvoor een bezoeker een tijdstip kan vastleggen. Zeg hoe lang het duurt en wanneer u ervoor open bent; de vrije tijden volgen uit uw agenda.",
  sitesBookingNoCalendarTitle: "Geen agenda om in te boeken",
  sitesBookingNoCalendarBody:
    "Een reservering is een afspraak in een van uw agenda’s, dus er moet een agenda zijn waarin u afspraken kunt zetten. Maak er één in Agenda en hij verschijnt hier.",
  sitesBookingSettings: "Deze dienst",
  sitesBookingSettingsHint:
    "Alles wat een bezoeker te zien krijgt. Wijzigingen bereiken de live website bij uw volgende publicatie.",
  sitesBookingName: "Wat er wordt geboekt",
  sitesBookingDescription: "Omschrijving",
  sitesBookingWhere: "Waar het plaatsvindt",
  sitesBookingWherePlaceholder: "Tweede verdieping, aanbellen",
  sitesBookingWhereLine: (place: string) => `Waar: ${place}`,
  sitesBookingCalendar: "Geboekt in",
  sitesBookingCalendarHint:
    "Afspraken worden in deze agenda gezet, en tijden waarop u daar al bezet bent worden nooit aangeboden.",
  sitesBookingCalendarReadOnly: (name: string) =>
    `${name} — alleen ter inzage met u gedeeld`,
  sitesBookingCalendarGone: "Agenda niet meer beschikbaar",
  sitesBookingCalendarGoneHint:
    "De agenda waarin deze dienst werd geboekt is niet meer bereikbaar — hij is verwijderd, of het delen is ingetrokken. Zolang u geen andere kiest, biedt de gepubliceerde pagina helemaal geen tijden meer aan.",
  sitesBookingOpenAgenda: "Agenda openen om de afspraken te beheren",
  sitesBookingLength: "Duur (minuten)",
  sitesBookingBuffer: "Pauze erna (minuten)",
  sitesBookingNotice: "Kortste opzegtermijn (minuten)",
  sitesBookingHorizon: "Vooruit open (dagen)",
  sitesBookingTimeZone: "Tijdzone",
  sitesBookingTimeZoneHint:
    "De klok waarop uw openingstijden staan, als IANA-naam zoals Europe/Brussels. Afspraken schuiven mee met de klok bij de zomer- en wintertijd.",
  sitesBookingHours: "Wanneer u ervoor open bent",
  sitesBookingHoursHint:
    "Een lege agenda is geen open dag. Deze vensters zijn wat wordt aangeboden; wat al in de agenda staat, gaat er daarna vanaf.",
  sitesBookingDay: "Dag",
  sitesBookingFrom: "Van",
  sitesBookingUntil: "Tot",
  sitesBookingAddWindow: "Venster toevoegen",
  sitesBookingRemoveWindow: (window: string) => `${window} verwijderen`,
  sitesBookingNoHours:
    "Nog geen openingstijden — er kan niets worden gereserveerd.",
  sitesBookingQuestions: "Wat u vraagt bij het reserveren",
  sitesBookingQuestionsHint:
    "Een naam en een e-mailadres worden altijd gevraagd en staan niet in deze lijst. Voeg alleen toe wat juist deze reservering nodig heeft.",
  sitesBookingQuestionLabel: "Vraag",
  sitesBookingQuestionLabelPlaceholder: "Telefoonnummer",
  sitesBookingQuestionKey: "Opgeslagen als",
  sitesBookingQuestionKind: "Soort antwoord",
  sitesBookingQuestionText: "Eén regel",
  sitesBookingQuestionLongText: "Meerdere regels",
  sitesBookingQuestionPhone: "Telefoonnummer",
  sitesBookingQuestionChoice: "Eén uit een lijst",
  sitesBookingQuestionOptions: "De antwoorden die u aanbiedt",
  sitesBookingQuestionOptionsPlaceholder: "Knippen, kleuren, allebei",
  sitesBookingQuestionRequired: "Moet worden ingevuld",
  sitesBookingAddQuestion: "Vraag toevoegen",
  sitesBookingRemoveQuestion: (question: string) =>
    `De vraag ${question} verwijderen`,
  sitesBookingActive: "Reserveringen hiervoor aannemen",
  sitesBookingActiveHint:
    "Uitgezet blijft de dienst precies zoals hij is en zegt de gepubliceerde pagina dat er voorlopig geen reserveringen worden aangenomen.",
  sitesBookingCreate: "Dienst aanmaken",
  sitesBookingSave: "Dienst opslaan",
  sitesBookingSaveFailed: "De reserveerbare dienst kon niet worden opgeslagen.",
  sitesBookingDelete: "Dienst verwijderen",
  sitesBookingDeleteConfirm: "Verwijderen",
  sitesBookingDeleteHint:
    "Afspraken die al in uw agenda staan blijven precies zoals ze zijn — hier wordt er geen enkele geannuleerd. Al gepubliceerde pagina’s blijven de dienst aanbieden tot u opnieuw publiceert.",
  sitesBookingDeleteFailed: "De reserveerbare dienst kon niet worden verwijderd.",
  sitesBookingMinutes: (minutes: number) => `${minutes} minuten`,
  sitesBookingOff: "Neemt geen reserveringen aan",
  sitesBookingPreview: "Wat een bezoeker ziet",
  sitesBookingPreviewHint:
    "Het aanbod zoals de gepubliceerde pagina het verwoordt. De vrije tijden zelf worden tegen uw agenda berekend op het moment dat iemand ernaar vraagt.",
  sitesBookingUnnamed: "Naamloze dienst",
  sitesBookingAsksNothingExtra:
    "Bezoekers wordt om hun naam en e-mailadres gevraagd.",
  sitesBookingAsksAlso: (questions: string) =>
    `Bezoekers wordt om hun naam en e-mailadres gevraagd, en om: ${questions}.`,
  sitesBookingPublishHint:
    "Hij verschijnt op de website zodra een pagina er een reserveringssectie voor draagt en u publiceert.",
  sitesBookingOffPreview:
    "Deze dienst staat uit, dus de pagina zegt dat er voorlopig geen reserveringen worden aangenomen.",
  sitesSectionBooking: "Reservering",
  sitesSectionBookingDesc:
    "Laat bezoekers een tijdstip bij u vastleggen, rechtstreeks in uw agenda.",
  sitesBookingSectionHeading: "Kop erboven",
  sitesBookingSectionChoose: "Wat hier gereserveerd kan worden",
  sitesBookingSectionNoServices: "Deze site heeft nog niets te reserveren",
  sitesBookingSectionNoServicesHint:
    "Een reserveerbare dienst zegt hoe lang hij duurt, wanneer u ervoor open bent en in welke agenda hij komt. Maak er één en deze sectie kan die aanbieden.",
  sitesBookingSectionOffOption: (name: string) =>
    `${name} (neemt geen reserveringen aan)`,
  sitesBookingSectionLength: (minutes: number) =>
    `Bezoekers kiezen een vrij tijdstip van ${minutes} minuten. Die tijden komen uit uw agenda op het moment dat ze het vragen, niet uit deze pagina.`,
  sitesBookingSectionOff:
    "Deze dienst staat uit, dus de gepubliceerde pagina zegt dat er voorlopig geen reserveringen worden aangenomen.",
  sitesBookingSectionGone:
    "De dienst die deze sectie aanbood bestaat niet meer. Kies een andere, anders wordt de volgende publicatie geweigerd.",
  sitesOrders: "Bestellingen",
  sitesOrdersLoadFailed:
    "De bestellingen konden niet worden geladen. Controleer uw verbinding en probeer het opnieuw.",
  sitesOrdersExport: "Exporteren als CSV",
  sitesOrdersExporting: "Bezig met exporteren...",
  sitesOrdersExportFailed: "De bestellingen konden niet worden geëxporteerd.",
  sitesNoOrdersTitle: "Nog geen bestellingen",
  sitesNoOrdersBody:
    "Zodra een gepubliceerde pagina een catalogus toont die bestellingen aanneemt, komt hier binnen wat bezoekers vragen — met de artikelen, hun gegevens en het totaal.",
  sitesOrderList: "Bestellingen",
  sitesOrderDetail: "Deze bestelling",
  sitesOrderFilter: "Tonen",
  sitesOrderFilterAll: "Alle",
  sitesOrderFilterOption: (label: string, count: number) =>
    `${label} (${count})`,
  sitesOrderFilterEmpty: "Geen bestellingen in deze staat.",
  sitesOrderStatus: "Hoe deze bestelling ervoor staat",
  sitesOrderStatusNew: "Nieuw",
  sitesOrderStatusConfirmed: "Bevestigd",
  sitesOrderStatusFulfilled: "Afgehandeld",
  sitesOrderStatusCancelled: "Geannuleerd",
  sitesOrderStatusFailed: "De bestelling kon niet worden verplaatst.",
  sitesOrderCatalog: "Uit",
  sitesOrderPhone: "Telefoon",
  sitesOrderItem: "Artikel",
  sitesOrderQuantity: "Aantal",
  sitesOrderUnitPrice: "Per stuk",
  sitesOrderLineTotal: "Regel",
  sitesOrderTotal: "Totaal",
  sitesOrderLinesCaption: "Wat er besteld is",
  sitesOrderLineNoPrice: "Op aanvraag",
  sitesOrderQuotedHint:
    "Een artikel zonder prijs telt niet mee in het totaal — geef zelf een prijs door in uw antwoord.",
  sitesOrderLineCount: (count: number) =>
    count === 1 ? "1 artikel" : `${count} artikelen`,
  sitesOrderDelete: "Bestelling verwijderen",
  sitesOrderDeleteConfirm: "Definitief verwijderen",
  sitesOrderDeleteHint:
    "Deze bestelling bevat iemands naam, telefoonnummer en wat die persoon vroeg. Verwijderen haalt dat allemaal weg — dit kan niet ongedaan worden gemaakt.",
  sitesOrderDeleteFailed: "De bestelling kon niet worden verwijderd.",
  sitesCollections: "Collecties",
  sitesCollectionsHint:
    "Maak van een alo Base-tabel herbruikbare kaarten voor uw website.",
  sitesConnectTable: "Tabel koppelen",
  sitesCollectionsLoading: "Collecties laden...",
  sitesCollectionsLoadFailed:
    "De collecties konden niet worden geladen. Controleer uw verbinding en probeer opnieuw.",
  sitesCollectionEmptyTitle: "Koppel uw eerste tabel",
  sitesCollectionEmptyBody:
    "Kies een alo Base, koppel de kolommen eenmaal en hergebruik de rijen op elke pagina.",
  sitesCollectionNoBasesTitle: "Maak eerst een alo Base",
  sitesCollectionNoBasesBody:
    "Collecties lezen rijen uit alo Base. Maak een Base in Drive en kom dan terug om die te koppelen.",
  sitesCollectionOpenDrive: "Drive openen",
  sitesCollectionName: "Collectienaam",
  sitesCollectionBase: "alo Base",
  sitesCollectionTable: "Tabel",
  sitesCollectionChooseBase: "Kies een Base",
  sitesCollectionChooseTable: "Kies een tabel",
  sitesCollectionRows: (count: number) =>
    count === 1 ? "1 rij" : `${count} rijen`,
  sitesCollectionConnectedTo: (base: string, table: string) =>
    `${base} / ${table}`,
  sitesCollectionSourceUnavailable:
    "Kies de Base en tabel waarvan de rijen op de website moeten verschijnen.",
  sitesCollectionEdit: (name: string) => `${name} bewerken`,
  sitesCollectionMapping: "Kolommen aan website-inhoud koppelen",
  sitesCollectionMappingHint:
    "De titel is verplicht. Al het andere is optioneel en kan later worden toegevoegd.",
  sitesCollectionOptional: "Optioneel",
  sitesCollectionNotMapped: "Niet tonen",
  sitesCollectionNoCompatibleField: "Deze tabel heeft een tekstkolom nodig",
  sitesCollectionTitleField: "Titel",
  sitesCollectionSlugField: "Paginapad",
  sitesCollectionSummaryField: "Samenvatting",
  sitesCollectionBodyField: "Inhoud",
  sitesCollectionImageField: "Afbeelding",
  sitesCollectionLinkField: "Link",
  sitesCollectionDateField: "Publicatiedatum",
  sitesCollectionSave: "Collectie opslaan",
  sitesCollectionSaving: "Opslaan...",
  sitesCollectionSaveFailed:
    "De collectie is niet opgeslagen. Er is niets gewijzigd; controleer de koppeling en probeer opnieuw.",
  sitesCollectionDisconnect: "Loskoppelen",
  sitesCollectionDisconnectConfirm: "Nu loskoppelen",
  sitesCollectionDisconnectHint:
    "De Base en alle rijen blijven in Drive staan.",
  sitesCollectionDisconnectFailed:
    "De collectie is nog gekoppeld. Verwijder haar van de pagina's die haar gebruiken en probeer opnieuw.",
  sitesCollectionPreview: "Huidige rijen",
  sitesCollectionPreviewHint:
    "Dit is precies wat de volgende publicatie uit Base zal lezen.",
  sitesCollectionPreviewLoading: "Huidige Base-rijen laden",
  sitesCollectionPreviewFailed:
    "Deze rijen konden niet worden bekeken. Herstel in Base de waarde die de server noemt en probeer opnieuw.",
  sitesCollectionPreviewSaveTitle: "Sla op om deze rijen te bekijken",
  sitesCollectionPreviewSaveBody:
    "Na het koppelen controleert dezelfde publicatieregel van de live site elke rij hier.",
  sitesCollectionPreviewEmptyTitle: "Deze tabel heeft nog geen volledige rijen",
  sitesCollectionPreviewEmptyBody:
    "Voeg in Base een titel aan een rij toe en die verschijnt hier automatisch.",
  sitesCollectionPreviewLinked: "Opent een link",
  sitesSectionCollection: "Collectie",
  sitesSectionCollectionDesc: "Een herbruikbaar raster met rijen uit alo Base.",
  sitesCollectionSectionHeading: "Sectiekop",
  sitesCollectionSectionChoose: "Te tonen collectie",
  sitesCollectionSectionNoConnections:
    "Koppel een tabel voordat u deze sectie toevoegt",
  sitesCollectionSectionNoConnectionsHint:
    "De collectie blijft herbruikbaar, zodat dezelfde Base meerdere pagina's kan voeden.",

  // Het blok met eigen code, afgesloten in een verzegeld kader (S2.14b).
  sitesSectionCustomCode: "Eigen code",
  sitesSectionCustomCodeDesc:
    "Uw eigen HTML, CSS en JavaScript, verzegeld in een kader zonder uitweg.",
  sitesCustomCodeBoundaryTitle: "Wat dit blok wel en niet kan",
  sitesCustomCodeBoundarySealed:
    "Het draait afgesloten van uw site: het kan de pagina eromheen niet lezen, uw bezoekers niet, en ook niet wat zij elders hebben ingevuld.",
  sitesCustomCodeBoundaryNoNetwork:
    "Het heeft geen netwerk. Er wordt niets van een ander adres geladen — geen insluiting, geen lettertype, geen meetscript — en juist daardoor blijft deze site zonder cookiebanner.",
  sitesCustomCodeBoundaryYours:
    "Het is uw code, gepubliceerd precies zoals u die hebt geschreven. Wij controleren niet wat die doet, en de assistent schrijft of wijzigt die niet.",
  sitesCustomCodeHeadingHint:
    "Wordt door de pagina boven het blok getoond, in de typografie van uw site. Laat leeg voor een blok dat op zichzelf staat.",
  sitesCustomCodeFrameTitle: "Wat dit blok is",
  sitesCustomCodeFrameTitleHint:
    "Wordt voorgelezen aan bezoekers met een schermlezer: \"Een timer voor de branding die nu loopt\", niet \"kader\".",
  sitesCustomCodeHtml: "Opmaak",
  sitesCustomCodeHtmlHint:
    "De inhoud van het blok. Het document eromheen — het beleid, de stijl- en scriptblokken — wordt voor u geschreven.",
  sitesCustomCodeCss: "Stijl",
  sitesCustomCodeCssHint: "Geldt alleen binnen dit blok. Optioneel.",
  sitesCustomCodeJs: "Script",
  sitesCustomCodeJsHint:
    "Draait alleen binnen dit blok, op het apparaat van de bezoeker.",
  sitesCustomCodeCapabilities: "Wat het blok mag",
  sitesCustomCodeCapabilitiesHint:
    "Alles staat uit tot u het aanzet, en alleen deze twee kunnen aan.",
  sitesCustomCodeScripts: "Een script uitvoeren",
  sitesCustomCodeScriptsHint:
    "Zonder dit is het blok opmaak en stijl: er wordt niets uitgevoerd, wat er ook in staat.",
  sitesCustomCodeScriptMissing:
    "Er is nog geen script om uit te voeren. Schrijf er een of zet dit uit — een recht zonder iets erachter wordt geweigerd.",
  sitesCustomCodeScriptDropped:
    "Uitgezet, dus het script hieronder wordt niet met het blok opgeslagen. Zet het weer aan om het te behouden.",
  sitesCustomCodeImages: "Afbeeldingen in de opmaak tonen",
  sitesCustomCodeImagesHint:
    "Voor een afbeelding die in de opmaak zelf staat. Een afbeelding van een adres kan nog steeds niet laden — gebruik daarvoor een afbeeldingssectie.",
  sitesCustomCodeHeight: "Hoogte op de pagina (pixels)",
  sitesCustomCodeHeightHint:
    "Een verzegeld blok kan van buitenaf niet worden opgemeten, dus geeft u de hoogte op. Tussen 40 en 2000.",
  sitesCustomCodeBytes: (used: number, max: number) => `${used} van ${max} bytes`,
  sitesCustomCodeBytesOver: (used: number, max: number) =>
    `${used} van ${max} bytes — te lang om op te slaan`,
  sitesCustomCodeTotalBytes: (used: number, max: number) =>
    `${used} van ${max} bytes in dit blok in totaal`,
  appLauncherAutoHint:
    "De apps die u het meest gebruikt, automatisch bijgehouden",
  meetTitle: "Vergadering",
  meetEyebrow: "Uw vergaderruimte",
  meetSubtitle: "Start een gesprek of stap binnen bij een vergadering die al bezig is.",
  meetHeroTitle: "Samen in één klik",
  meetHeroText: "Microfoon aan, camera naar keuze. Controleer beide voordat iemand u ziet of hoort.",
  meetHappeningNow: "Nu bezig",
  meetHappeningHint: "Vergaderingen waaraan u zonder link kunt deelnemen.",
  meetLiveCount: (count: number) => count === 1 ? "1 vergadering" : `${count} vergaderingen`,
  meetReady: "Klaar",
  meetStartedAt: (time: string) => `Begonnen om ${time}`,
  meetInstantTitle: "Directe vergadering",
  meetNothingLive: "Er zijn geen vergaderingen bezig",
  meetWhereFrom:
    "Vergaderingen beginnen meestal waar de mensen zijn — in een gesprek of op een agenda-uitnodiging. Alles wat loopt en waaraan u kunt deelnemen, verschijnt hier.",
  meetUntitled: "Naamloze vergadering",
  meetNotStarted: "Nog niet begonnen",
  meetAddToEvent: "Vergadering toevoegen",
  meetStart: "Een vergadering starten",
  meetStartNow: "Een vergadering starten",
  meetStarting: "Bezig met starten…",
  meetStartFailed: "De vergadering kon niet worden gestart. Controleer uw verbinding en probeer het opnieuw.",
  meetLoading: "Vergaderingen laden",
  meetLoadFailed: "Vergaderingen konden niet worden geladen",
  meetLoadFailedHint: "Controleer uw verbinding en probeer het opnieuw. U kunt nog steeds een nieuwe vergadering starten.",
  meetRetry: "Opnieuw proberen",
  meetBack: "Terug naar Meet",
  meetStartedHere: "is een vergadering gestart in dit gesprek",
  meetJoin: "Deelnemen aan de vergadering",
  meetLive: "Vergadering bezig",
  meetJoinNow: "Nu deelnemen",
  meetReadyGreeting: (name: string) => name ? `Hallo ${name}` : "Hallo",
  meetReadyTitle: "U bent klaar om deel te nemen",
  meetReadyBody: "Controleer uw camera en microfoon voordat u deelneemt.",
  meetReadySafetyTitle: "Uw vergadering is veilig",
  meetReadySafetyBody: "Alleen genodigden en deelnemers die de host toelaat, kunnen deelnemen.",
  meetMicrophone: "Microfoon",
  meetCamera: "Camera",
  meetJoining: "Bezig met deelnemen…",
  meetLeave: "Verlaten",
  meetPresentingTitle: "U presenteert",
  meetPresentingBody: "Alle anderen zien uw gedeelde scherm. U ziet deze rustige herinnering in plaats van een eindeloze spiegeling.",
  meetClose: "Sluiten",
  meetJoinFailed: "Deelnemen aan die vergadering is niet gelukt.",
  meetJoinProblemTitle: "We konden u niet verbinden",
  meetUnavailableTitle: "Meet heeft nog één verbinding nodig",
  meetRaiseHand: "Hand opsteken",
  meetLowerHand: "Hand laten zakken",
  meetReact: "Een reactie sturen",
  meetInvite: "Uitnodigen",
  meetInviteTitle: "Neem deel aan mijn alo-vergadering",
  meetInviteText: "Gebruik deze alo-link om deel te nemen.",
  meetChatEmptyTitle: "De ruimte luistert",
  meetChatEmptyBody: "Deel een gedachte, een link of het detail dat iedereen na het gesprek nodig heeft.",
  meetJoinPlaceholder: "Voer een vergadercode of alo-link in",
  meetJoinShort: "Deelnemen",
  meetNew: "Nieuwe vergadering",
  meetYourSpaceLead: "Uw",
  meetYourSpaceAccent: "vergaderruimte",
  meetHeroNewTitle: "Kom met één klik samen",
  meetHeroNewText: "Hoogwaardige gesprekken met schermdelen, chat, reacties en een apparaatcontrole voordat iemand u ziet of hoort.",
  meetSchedule: "Plannen",
  meetJoinInputInvalid: "Voer een geldige alo-vergaderlink of vergadercode in.",
  meetUpcoming: "Aankomende vergaderingen",
  meetUpcomingHint: "Wat volgens uw agenda hierna komt.",
  meetCalendarUntitled: "Agenda-item zonder titel",
  meetSafetyTitle: "U houdt de toegang onder controle",
  meetSafetyBody: "De werkruimte controleert de toegang voordat een mediatoken wordt uitgegeven. Een vergadercode omzeilt de autorisatie nooit.",
  meetTodaySchedule: "De planning van vandaag",
  meetOpenAgenda: "Agenda openen",
  meetNoEventsToday: "Er staat vandaag verder niets gepland.",
  meetViewAgenda: "Volledige Agenda bekijken",
  meetQuickActions: "Snelle acties",
  meetLinkCopied: "Link gekopieerd",
  meetSomeone: "Iemand",
  meetHandsRaised: (names: string) => `Hand opgestoken: ${names}`,
  meetNoEngine:
    "Vergaderingen zijn nog niet ingeschakeld voor deze werkruimte. De vergadering is vastgelegd en iedereen die is uitgenodigd kan haar zien — er is alleen nog geen plek om haar te houden totdat een beheerder de vergaderserver instelt.",
  agendaAgenda: "Agenda",
  agendaCreateEvent: "Afspraak maken",
  agendaDay: "Dag",
  agendaDescriptionPlaceholder: "Voeg notities, agenda of andere details toe…",
  agendaEditEventSubtitle: "Werk de details van uw afspraak bij",
  agendaLocationPlaceholder: "Voeg een locatie of videogesprekslink toe",
  agendaMyCalendars: "Mijn agenda's",
  agendaNewEventSubtitle: "Maak een nieuwe afspraak in uw agenda",
  agendaNothingUpcoming: "Niets gepland.",
  agendaOtherCalendars: "Andere agenda's",
  agendaTomorrow: "Morgen",
  agendaUntitledEvent: "Naamloze afspraak",
  agendaUpcoming: "Binnenkort",
  driveActions: "Acties",
  driveAdd: "Toevoegen",
  driveAddMemberLabel: "E-mailadres",
  driveAddMemberPlaceholder: "Voeg iemand toe via e-mail",
  driveColModified: "Gewijzigd",
  driveColName: "Naam",
  driveColSize: "Grootte",
  driveCopy: "Een kopie maken",
  driveCopyTo: "Kopiëren naar…",
  driveCurrent: "Huidige",
  driveDeleteForever: "Definitief verwijderen",
  driveDestHint: "Het item neemt de toegang over van de plek waar u het zet.",
  driveDownload: "Downloaden",
  driveKindDoc: "Document",
  driveKindExcel: "Excel-spreadsheet",
  driveKindFolder: "Map",
  driveKindSheet: "Sheet",
  driveKindSlides: "Slides (PowerPoint)",
  driveKindWord: "Word-document",
  driveMemberError:
    "Kon die persoon niet toevoegen — controleer het e-mailadres en uw rol.",
  driveMemberRoleLabel: "Rol",
  driveMembers: "Leden",
  driveMove: "Verplaatsen",
  driveMoveTo: "Verplaatsen naar…",
  driveMyFiles: "Mijn bestanden",
  driveNew: "Nieuw",
  driveNewBase: "Nieuwe base",
  driveNewBasePrompt: "Geef de nieuwe base een naam",
  driveNewDoc: "Nieuw doc",
  driveNewDocPrompt: "Geef het nieuwe doc een naam",
  driveNewFolder: "Nieuwe map",
  driveNewFolderPrompt: "Geef de nieuwe map een naam",
  driveNewSheetPrompt: "Geef de nieuwe sheet een naam",
  driveNewSpace: "Nieuwe Space",
  driveNewSpacePrompt: "Geef de nieuwe Space een naam",
  driveNoVersions: "Geen eerdere versies.",
  driveOpen: "Openen",
  driveRemoveMember: "Verwijderen",
  driveRemoveMemberFor: (who: string): string => `${who} verwijderen`,
  driveRename: "Naam wijzigen",
  driveRenamePrompt: "Nieuwe naam",
  driveRestore: "Herstellen",
  driveSpaces: "Spaces",
  driveTrash: "Prullenbak",
  driveTrashAction: "Naar prullenbak verplaatsen",
  driveUpload: "Uploaden",
  driveUploading: "Bezig met uploaden…",
  driveVersionHistory: "Versiegeschiedenis",
  agendaEventCount: (n: number) => (n === 1 ? "1 afspraak" : `${n} afspraken`),
  driveMembersOf: (name: string) => `Leden van ${name}`,
  driveNameNew: (kind: string): string =>
    `Geef uw ${kind.toLowerCase()} een naam`,
  drivePurgeConfirm: (name: string) =>
    `“${name}” definitief verwijderen? Dit kan niet ongedaan worden gemaakt.`,
  driveRemoveMemberConfirm: (who: string) =>
    `${who} uit deze Space verwijderen?`,
  driveTrashConfirm: (name: string) =>
    `“${name}” naar de prullenbak verplaatsen?`,
  driveRole: (role: string) =>
    role === "manager" ? "Beheerder" : role === "editor" ? "Bewerker" : "Lezer",
  agentActAmIFree: "Controleer op overlap",
  agentActArchive: "Archiveren",
  agentActCatchUp: "Lees wat er is gezegd",
  agentActDraft: "Nieuwe e-mail",
  agentActEvent: "Toevoegen aan agenda",
  agentActFindContact: "Een contact opzoeken",
  agentActFindFile: "Zoek in uw Drive",
  agentActFindInChat: "Gesprekken doorzoeken",
  agentActFlag: "Markeren",
  agentActMarkRead: "Markeren als gelezen",
  agentActMarkUnread: "Markeren als ongelezen",
  agentActMove: "Verplaatsen naar map",
  agentActReply: "Beantwoorden",
  agentActSend: "E-mail verzenden",
  agentActSnooze: "Sluimeren",
  agentActTask: "Taak aanmaken",
  agentActTrash: "Naar prullenbak verplaatsen",
  agentActUnflag: "Markering verwijderen",
  agentActWhatsOn: "Lees uw agenda",
  agentFieldDue: "Deadline",
  agentFieldEmail: "E-mail",
  agentFieldEvent: "Afspraak",
  agentFieldFolder: "Map",
  agentFieldLookingFor: "Op zoek naar",
  agentFieldReplyTo: "Als antwoord op",
  agentFieldRoom: "Gesprek",
  agentFieldSubject: "Onderwerp",
  agentFieldTask: "Taak",
  agentFieldTo: "Aan",
  agentFieldUntil: "Tot",
  agentFieldWhen: "Wanneer",
  agentNoSubject: "(geen onderwerp)",
  agentSendButton: "Verzenden",
  agentSendCaution:
    "Hiermee wordt de e-mail nu verzonden — dat kan niet ongedaan worden gemaakt.",
  chatAddReaction: "Een reactie toevoegen",
  chatAgentAddFailed: "Die agent kon niet worden toegevoegd.",
  chatAgentNothingYet: "Nog niets gevraagd",
  chatAgentRemoveFailed: "Die agent kon niet worden verwijderd.",
  chatAgentTag: "agent",
  chatAgentsAvailable: "Beschikbaar om toe te voegen",
  chatAgentsHere: "Agents in dit gesprek",
  chatArchiveAction: "Kanaal archiveren",
  chatArchiveConfirm: "Archiveren",
  chatArchiveFailed: "Dat kanaal kon niet worden gearchiveerd.",
  chatArchiveWarning:
    "Er wordt niets verwijderd. De geschiedenis blijft leesbaar, maar niemand kan hier nog berichten plaatsen.",
  chatArchived: "Gearchiveerd",
  chatArchivedNote:
    "Dit kanaal is gearchiveerd. De geschiedenis blijft hier te lezen, maar er kan niets nieuws worden verzonden.",
  chatAttach: "Een bestand bijvoegen",
  chatAttachFailed: "Dat bestand kon niet worden gedeeld.",
  chatBackToList: "Terug naar gesprekken",
  chatBeginningDm: "Dit is het begin van uw gesprek",
  chatBold: "Vet",
  chatBrowse: "Kanalen verkennen",
  chatBrowseFailed: "Die kanalen konden niet worden getoond.",
  chatBulletList: "Opsomming",
  chatClose: "Sluiten",
  chatCodeBlock: "Codeblok",
  chatCodeBlockHint: "Voeg een opgemaakt blok voor code of opdrachten in.",
  chatFormulaHint: "Voeg een wiskundige formule in.",
  chatFormatting: "Tekstopmaak",
  chatComposerLabel: "Een bericht schrijven",
  chatCreate: "Aanmaken",
  chatCreateFailed: "Dat kanaal kon niet worden aangemaakt.",
  chatDecideFailed: "Daarover kon niet worden beslist.",
  chatDirectMessage: "Direct bericht",
  chatDmFailed: "Dat gesprek kon niet worden gestart.",
  chatDropFiles: "Laat los om vanaf uw computer te delen",
  chatEditAction: "Bewerken",
  chatEditCancel: "Annuleren",
  chatEditFailed: "Die bewerking kon niet worden opgeslagen.",
  chatEditLabel: "Dit bericht bewerken",
  chatEditSave: "Opslaan",
  chatEdited: "bewerkt",
  chatEmojiNone: "Geen emoji komt overeen.",
  chatEmojiSearch: "Emoji zoeken",
  chatFileTrashed: "in de prullenbak van Drive",
  chatFindPerson: "Een collega zoeken",
  chatFindPersonHint: "Typ minstens twee letters van hun adres.",
  chatFormatHint: "tekst",
  chatFormula: "Formule",
  chatInlineCode: "Code",
  chatInsertEmoji: "Emoji",
  chatItalic: "Cursief",
  chatJoin: "Deelnemen",
  chatJoinFailed: "Deelnemen aan dat kanaal is niet gelukt.",
  chatJoined: "Openen",
  chatJumpTo: "Naar een gesprek springen",
  chatLoadFailed: "Die gesprekken konden niet worden geladen.",
  chatLoading: "Laden…",
  chatMembersAndAgents: "Leden en agents",
  chatNewChannel: "Nieuw kanaal",
  chatNewChannelPlaceholder: "bijv. productlancering",
  chatNewChannelPrompt:
    "Geef het een korte, duidelijke naam — mensen vinden het daarmee.",
  chatNewDm: "Nieuw gesprek",
  chatNewMessages: "Nieuwe berichten",
  chatNoAgentsHere:
    "Nog geen agents hier. Voeg er een toe en noem die bij naam.",
  chatNoChannelsHint:
    "Maak een kanaal voor een team of een onderwerp, dan ziet iedereen daarin dezelfde geschiedenis.",
  chatNoChannelsLead: "Nog geen gesprekken",
  chatNoMessagesYet: "Nog geen berichten — zeg als eerste iets.",
  chatNoRoom: "Geen gesprek komt overeen.",
  chatNoRoomOpenHint: "Kies links een kanaal of maak een nieuw kanaal.",
  chatNoRoomOpenLead: "Kies een gesprek",
  chatNobodyFound: "Niemand hier komt overeen.",
  chatNothingToJoin: "Nog geen openbare kanalen in deze werkruimte.",
  chatOlder: "Eerdere berichten tonen",
  chatOpenFile: "Openen in Drive",
  chatOwner: "eigenaar",
  chatPeopleFailed: "Die zoekopdracht kon niet worden uitgevoerd.",
  chatPeopleHere: "Mensen",
  chatProposalNotYours:
    "Alleen wie het heeft gevraagd kan dit goedkeuren — het zou met hun toegang worden uitgevoerd.",
  chatQuoteAction: "Citeren",
  chatReactFailed: "Die reactie kon niet worden opgeslagen.",
  chatRename: "Kanaal hernoemen",
  chatRenameFailed: "Dat kanaal kon niet worden hernoemd.",
  chatRenamePrompt: "Iedereen in het kanaal ziet de nieuwe naam.",
  chatRenameSave: "Hernoemen",
  chatReplyInThread: "Hier beantwoorden",
  chatReplyHere: "Hier beantwoorden",
  chatReplyPrivately: "Privé beantwoorden",
  chatReplyingHere: "Hier beantwoorden",
  chatReplyingPrivately: (who: string): string => `Privé antwoorden aan ${who}`,
  chatCancelReply: "Antwoord annuleren",
  chatSearchClear: "Zoekopdracht wissen",
  chatSearchFailed: "Die zoekopdracht kon niet worden uitgevoerd.",
  chatSearchNothing: "Niets gevonden.",
  chatSearchPlaceholder: "Berichten zoeken",
  chatSectionArchived: "Gearchiveerd",
  chatSectionChannels: "Kanalen",
  chatFilterAll: "Alles",
  chatFilterUnread: "Ongelezen",
  chatFilterThreads: "Threads",
  chatFilterMentions: "Vermeldingen",
  chatCompose: "Opstellen",
  chatSectionDirect: "Directe berichten",
  chatSend: "Verzenden",
  chatSendFailed:
    "Dat bericht kon niet worden verzonden — uw tekst staat er nog.",
  chatShare: "Iets delen",
  chatShareAsk: "Vraag het alo",
  chatShareAskHint: "Antwoorden uit uw hele werkruimte",
  chatShareFile: "Bestand uit Drive",
  chatShareFileHint: "Een verwijzing, geen kopie — het blijft in Drive",
  chatShareMention: "Iemand vermelden",
  chatShareMentionHint: "Mensen en agents in dit gesprek",
  chatStop: "Stoppen",
  chatThread: "Thread",
  chatThreadClose: "Thread sluiten",
  chatThreadEmpty: "Nog geen antwoorden — begin deze.",
  chatThreadFailed: "Die thread kon niet worden geladen.",
  chatThreadPlaceholder: "Antwoorden…",
  chatToday: "Vandaag",
  chatWhoIsHere: "Wie is hier",
  chatWithdrawAction: "Intrekken",
  chatWithdrawFailed: "Dat bericht kon niet worden ingetrokken.",
  chatWithdrawn: "Dit bericht is ingetrokken.",
  chatYesterday: "Gisteren",
  docSaving: "Opslaan…",
  chatAgentAdd: (handle: string): string =>
    `@${handle} aan dit gesprek toevoegen`,
  chatAgentRemove: (handle: string): string => `@${handle} verwijderen`,
  chatArchiveTitle: (name: string): string => `${name} archiveren?`,
  chatBeginning: (name: string): string => `Dit is het begin van ${name}`,
  chatChannelActions: (name: string): string => `Acties voor ${name}`,
  chatComposerPlaceholder: (room: string): string => `Bericht aan ${room}`,
  chatThinking: (handle: string): string => `@${handle} denkt na`,
  chatUnstage: (name: string): string => `${name} verwijderen`,
  chatReplies: (count: number): string =>
    count === 1 ? "1 antwoord" : `${count} antwoorden`,
  chatMentionsYou: (count: number): string =>
    count === 1 ? "1 bericht vermeldt u" : `${count} berichten vermelden u`,
  chatProposalSettled: (state: string): string =>
    state === "approved" ? "Goedgekeurd en uitgevoerd." : `Status: ${state}.`,
  chatAgentRecord: (answers: number, actions: number): string => {
    const said = answers === 1 ? "1 antwoord" : `${answers} antwoorden`;
    if (actions === 0) return said;
    return `${said} · ${actions === 1 ? "1 actie" : `${actions} acties`} goedgekeurd`;
  },
  agentActWhoIsOff: "Zie wie afwezig is",
  agentWhoIsOffNote:
    "Leest het teamoverzicht van afwezigheden dat iedereen hier al ziet: wie afwezig is, en op welke dagen. Het verandert niets, boekt niets en waarschuwt niemand.",
  agentWhoIsOffAway: "Afwezig",
  agentWhoIsOffNobody: "Niemand",
  agentWhoIsOffFooter:
    "Alleen namen en dagen — goedgekeurd verlof vermeldt nooit waarom iemand afwezig is. Wie er niet bij staat, kan nog steeds weg zijn om een reden die hier niet onder valt.",
  agentWhoIsOffCount: (count: number): string =>
    count === 1 ? "1 persoon" : `${count} personen`,
  agentWhoIsOffDays: (count: number): string =>
    count === 1 ? "1 dag" : `${count} dagen`,
  baseAddField: "Veld toevoegen",
  baseAddView: "Weergave toevoegen",
  baseBoardNeedsSelect:
    "Voeg een bordweergave toe die is gegroepeerd op een Keuze-veld om dit te gebruiken.",
  baseByDate: "Op datum…",
  baseCalendarNeedsDate:
    "Voeg een kalenderweergave toe op basis van een Datum-veld om dit te gebruiken.",
  baseChoicesPlaceholder: "Keuzes, gescheiden door komma's",
  baseFieldName: "Veldnaam",
  baseGroupBy: "Groeperen op…",
  baseLink: "Koppeling",
  baseLinkNoRecords: "De gekoppelde tabel heeft nog geen records.",
  baseLinkNoTable: "Geen gekoppelde tabel ingesteld.",
  baseLinkTarget: "Gekoppelde tabel…",
  baseNewRow: "Nieuwe rij",
  baseNewTable: "Nieuwe tabel",
  baseNoChoices: "Nog geen keuzes — voeg ze toe op het veld.",
  basePersonPlaceholder: "email@…",
  baseTypeCheckbox: "Selectievakje",
  baseTypeDate: "Datum",
  baseTypeLink: "Koppeling naar tabel",
  baseTypeMultiselect: "Meerkeuze",
  baseTypeNumber: "Getal",
  baseTypePerson: "Persoon",
  baseTypeSelect: "Keuze",
  baseTypeText: "Tekst",
  baseUncategorised: "Niet gecategoriseerd",
  baseUntitledRecord: "Naamloos",
  baseViewBoard: "Bord",
  baseViewCalendar: "Kalender",
  baseViewGallery: "Galerij",
  baseViewGrid: "Raster",
  brandEuBadgeDrive: "Uw bestanden, geen lock-in",
  brandHeadlineDrive: "Uw bestanden.\nUw mappen.\nUw regels.",
  brandSubtitleDrive:
    "Bestanden, mappen en documenten op één plek — gedeeld op basis van waar ze staan, en altijd van u.",
  cancel: "Annuleren",
  close: "Sluiten",
  datePickerClear: "Wissen",
  datePickerToday: "Vandaag",
  homeGoToTasks: "Taken openen",
  homeMyTasks: "Mijn taken",
  homeNewTask: "Nieuwe taak",
  homeNoEventsToday: "Vandaag niets in uw agenda.",
  homeNoTasks: "Niets te doen. U bent helemaal bij.",
  homeNotifications: "Meldingen",
  homeSearchPlaceholder: "Zoek in e-mail, afspraken, taken…",
  homeStatTasks: "Taken voor vandaag",
  homeSubtitle: "Dit staat er vandaag te gebeuren.",
  homeTaskOverdue: "Te laat",
  homeTaskToday: "Vandaag",
  homeTodaysCalendar: "Agenda van vandaag",
  homeViewAllTasks: "Alle taken bekijken",
  homeViewCalendar: "Agenda bekijken",
  homeViewFullCalendar: "Volledige agenda bekijken",
  homeViewTasks: "Taken bekijken",
  moduleHr: "Mensen",
  officeUnavailable:
    "Dit document kon niet worden geopend om te bewerken. Probeer het opnieuw of download het.",
  pickerAttach: "Bijvoegen",
  pickerEmpty: "Hier is nog niets.",
  pickerLoadFailed: "Die map kon niet worden geopend.",
  pickerLoading: "Laden…",
  pickerMyDrive: "Mijn Drive",
  pickerNonePicked: "Geen bestanden gekozen",
  pickerPersonalNotice:
    "Bestanden in Mijn Drive zijn alleen van u — mensen in het gesprek kunnen ze niet openen. Gebruik een Space om te delen.",
  pickerPlaces: "Waar te zoeken",
  pickerTitle: "Kies een bestand",
  specBcHeading: "Randvoorwaarden",
  specLead1: "De stationaire flux wordt beschreven door",
  specLead2: "over de grens.",
  specMid:
    "waarbij k de warmtegeleidingscoëfficiënt is en r₁, r₂ de binnen- en buitenstraal zijn. Als we de gemeten waarden invullen:",
  specRefLead: "Door",
  specRefMid: "te combineren met de waarden in",
  specRefTail: "krijgen we de onderstaande getallen.",
  specSubtitle: "Technische specificatie · Rev. 3",
  specTitle: "Warmteoverdracht in het Coateq-paneel",
  tblSymbol: "Symbool",
  tblValue: "Waarde",
  taskAddAttachment: "Bijlage toevoegen",
  taskAddBlocker: "Blokkade toevoegen",
  taskAddLabel: "Label toevoegen",
  taskAllTasks: "Alle taken",
  taskAssigneeYou: "U",
  taskAttachments: "Bijlagen",
  taskBlockedBy: "Geblokkeerd door",
  taskCalendar: "Kalender",
  taskCancel: "Annuleren",
  taskColAssignee: "Toegewezen aan",
  taskColDue: "Vervaldatum",
  taskColName: "Taaknaam",
  taskColPriority: "Prioriteit",
  taskColProject: "Project",
  taskColReview: "Beoordeling",
  taskCompactRows: "Compacte rijen",
  taskCreate: "Taak aanmaken",
  taskCreateAnother: "Nog een taak aanmaken",
  taskCreateFirst: "Maak uw eerste taak aan",
  taskCreateLabel: "Aanmaken",
  taskDownload: "Downloaden",
  taskEmptyBody: "Alles is klaar. Begin met het aanmaken van uw eerste taak.",
  taskEmptyTitle: "Nog geen taken 👋",
  taskFiles: "Bestanden",
  taskFilesEmpty: "Nog geen bestanden. Voeg er een toe vanuit een taak.",
  taskFilter: "Filteren",
  taskFollow: "Volgen",
  taskFollowers: "Volgers",
  taskGroup: "Groeperen",
  taskGroupAssignee: "Toegewezen aan",
  taskGroupNone: "Geen",
  taskGroupPriority: "Prioriteit",
  taskGroupProject: "Project",
  taskGroupStatus: "Status",
  taskLabelsTitle: "Labels",
  taskLeave: "Taak verlaten",
  taskMarkDone: "Markeren als klaar",
  taskMarkNotDone: "Markeren als niet klaar",
  taskNamePlaceholder: "bijv. Landingspagina ontwerpen",
  taskNew: "Nieuwe taak",
  taskNewLabelPlaceholder: "Nieuw label…",
  taskNewSubtitle: "Maak een taak aan en houd overzicht.",
  taskNewTaskPrompt: "Naam voor de nieuwe taak",
  taskNoBlockerCandidates: "Geen andere taken om van af te hangen",
  taskOnlyMine: "Alleen mijn taken",
  taskOptions: "Opties",
  taskOvByAssignee: "Taken per persoon",
  taskOvCompleted: "Voltooid",
  taskOvCompletedLabel: "Voltooid",
  taskOvNobody: "Niet toegewezen",
  taskOvProgress: "Voortgang",
  taskOvTotal: "Totaal",
  taskOvUpcoming: "Aankomende taken",
  taskOvViewAll: "Alles bekijken",
  taskOverview: "Overzicht",
  taskSearchPlaceholder: "Zoek taken, projecten…",
  taskShowCompleted: "Voltooide tonen",
  taskSort: "Sorteren",
  taskSortCreated: "Nieuwste",
  taskSortDue: "Vervaldatum",
  taskSortManual: "Handmatig",
  taskSortName: "Naam",
  taskSortPriority: "Prioriteit",
  taskTimeline: "Tijdlijn",
  taskUnassigned: "Niet toegewezen",
  taskUnscheduled: "Geen vervaldatum",
  taskUploading: "Bezig met uploaden…",
  userAccountantBadge: "Boekhouder",
  userAccountantHint:
    "Leest de boeken — rapporten, onkostengoedkeuringen en het afsluiten van een periode — en kan facturen en deals inzien zonder ze te wijzigen. Geen adminconsole, en geen toegang tot andermans e-mail of bestanden.",
  userAccountantRole: "Boekhouder",
  userRoles: "Rollen",
  pickerPicked: (count: number, max: number): string =>
    `${count} van ${max} gekozen`,
  taskOvTasksTotal: (n: number) => `${n} taken in totaal`,
  hrActions: "Beslissing",
  hrAddCandidate: "Een kandidaat toevoegen",
  hrAddNote: "Notitie toevoegen",
  hrAlsoAway: "Dan al afwezig",
  hrApprovalsEmptyBody:
    "Verlof, onkostennota's en urenweken die mensen indienen komen hier samen binnen, oudste eerst — zodat niemand hoeft te wachten omdat zijn verzoek in de module stond die u het laatst opende.",
  hrApprovalsEmptyTitle: "Er wacht niets",
  hrApprovalsNoneBody:
    "Hier wachten verlof, onkostennota's en urenweken op wie erover beslist. U ziet dit zodra iemand aan u rapporteert, of wanneer u de boeken bijhoudt.",
  hrApprovalsNoneTitle: "Er komt niets bij u ter beslissing",
  hrApprovalsWidgetLabel: "wachtend",
  hrApprovalsWidgetTitle:
    "Verlof, nota's en weken die op uw beslissing wachten",
  hrApprove: "Goedkeuren",
  hrAskForLeave: "Verlof aanvragen",
  hrAskSubmit: "Aanvragen",
  hrAskSubtitle:
    "De dagen gaan af van het saldo voor de soort die u kiest, berekend op uw eigen werkpatroon — u typt nooit zelf een aantal dagen.",
  hrAwayCalendar: "Wie is afwezig, per dag",
  hrBalanceBooked: "Geboekt",
  hrBalanceLeft: "over",
  hrBalanceTaken: "Opgenomen",
  hrBalanceThisYear: "Dit jaar",
  hrBalanceWaiting: "Wachtend",
  hrCancel: "Annuleren",
  hrCancelLeave: "Annuleren",
  hrCandidate: "Kandidaat",
  hrCandidateSubtitle:
    "Wat er in de sollicitatie stond. Niets hiervan wordt door een machine gelezen — geen selectie, geen rangschikking, geen score.",
  hrClearSearch: "Zoekopdracht wissen",
  hrClose: "Sluiten",
  hrCloseOpening: "Ronde sluiten",
  hrClosedNotice:
    "Deze ronde is gesloten. Het bord blijft leesbaar en de mensen erop kunnen nog worden verplaatst — maar er kan niemand nieuw bij.",
  hrContact: "Contact",
  hrCreate: "Aanmaken",
  hrCv: "CV",
  hrCvAttach: "Een cv toevoegen",
  hrCvDownload: "Het cv downloaden",
  hrCvFailed: "Dat bestand kon niet worden gedownload.",
  hrCvHint:
    "Opgeslagen in het HR-gedeelte, waar alleen HR het kan openen. Niets leest het — geen selectie, geen rangschikking, geen score.",
  hrCvNone: "Geen cv aanwezig.",
  hrCvRemove: "Het cv van dit dossier halen",
  hrCvReplace: "Het cv vervangen",
  hrCvTrashed: "Het cv dat aanwezig was, is naar de HR-prullenbak verplaatst.",
  hrCvUploadFailed:
    "Dat bestand is niet geüpload, dus er is niets opgeslagen. Probeer het opnieuw, of sla de gegevens zonder cv op.",
  hrDirectoryEmptyBody:
    "Zodra HR de eerste persoon opschrijft, vindt iedereen hier zijn collega's — wie ze zijn, hoe u ze bereikt, en aan wie ze rapporteren.",
  hrDirectoryEmptyTitle: "Er staat nog niemand in het adresboek",
  hrDirectorySearch: "Mensen zoeken",
  hrDirectoryViews: "Hoe u het adresboek leest",
  hrEditCandidate: "Gegevens bewerken",
  hrEditOpening: "Rol bewerken",
  hrErase: "Dit dossier wissen",
  hrFieldEmail: "E-mail",
  hrFieldEmployment: "Dienstverband",
  hrFieldFamilyName: "Achternaam",
  hrFieldFirstDay: "Eerste vrije dag",
  hrFieldGivenName: "Voornaam",
  hrFieldJobTitle: "Functie",
  hrFieldLastDay: "Laatste vrije dag",
  hrFieldLocation: "Locatie",
  hrFieldName: "Naam",
  hrFieldPhone: "Telefoon",
  hrFieldRetainUntil: "Bewaren tot",
  hrFieldRole: "Rol",
  hrFieldSource: "Waar ze vandaan komen",
  hrFieldStartedOn: "Begint op",
  hrFieldTeam: "Team",
  hrFieldWorkEmail: "Werkmail",
  hrFigure: "Bedrag",
  hrHire: "Toevoegen aan de directory",
  hrHireEmailHint:
    "Hun werkadres, als dat al bekend is. Het kan later worden toegevoegd.",
  hrHireNameHint:
    "Afgeleid van de naam op de sollicitatie. Corrigeer het als het verkeerd is gesplitst.",
  hrHireNoAccount:
    "Dit schrijft een dossier in Mensen. Het maakt geen login of mailbox aan — dat doet een beheerder, en de onboardingchecklist heeft daar een taak voor.",
  hrHireNoKind: "Niet vermeld",
  hrHireStartHint:
    "De dag waarop hun voorwaarden ingaan. Elk verlofsaldo wordt daarvandaan geteld.",
  hrHireSubmit: "Toevoegen aan de directory",
  hrHireSubtitle:
    "Hun personeelsdossier en de voorwaarden waarmee ze beginnen. Alles is ingevuld vanuit de sollicitatie en de rol — corrigeer wat niet klopt.",
  hrHired: "Ze hebben de baan aangenomen",
  hrHiredExplainer:
    "Iemand naar Aangenomen verplaatsen legt vast wat er is gebeurd. Ze in de directory schrijven is een aparte handeling, die u hier doet.",
  hrHolidaysInside: "Er valt een feestdag binnen deze data; die telt niet mee.",
  hrIncludeClosed: "Gesloten rondes meenemen",
  hrIncludeLeavers: "Mensen die zijn vertrokken meenemen",
  hrKindApprentice: "Leerwerkplek",
  hrKindContractor: "Zelfstandige",
  hrKindFixedTerm: "Bepaalde tijd",
  hrKindIntern: "Stage",
  hrKindPartTime: "Deeltijd",
  hrKindPermanent: "Vast",
  hrLastDayHint: "De dag waarop u terugkomt hoort er niet bij.",
  hrLeaveApproved: "Geboekt",
  hrLeaveCancelled: "Geannuleerd",
  hrLeaveDays: "Dagen",
  hrLeaveEmptyBody:
    "Vraag hier een dag of twee weken aan. U ziet wat het uw saldo kost voordat iemand beslist, en wie er in die dagen al weg is.",
  hrLeaveEmptyTitle: "U hebt nog geen verlof aangevraagd",
  hrLeaveKind: "Soort",
  hrLeaveNoneShownBody:
    "Er is verlof vastgelegd, maar niets daarvan heeft de status waar u om vroeg.",
  hrLeaveNoneShownTitle: "Niets met die status",
  hrLeaveRejected: "Niet akkoord",
  hrLeaveRequested: "Wachtend",
  hrLeaveShow: "Tonen",
  hrLeaveState: "Status",
  hrLeaveTeamEmptyBody:
    "Wanneer iemand die aan u rapporteert vrije dagen aanvraagt, komt dat hier binnen en in uw goedkeuringen — met de data, wat het hun saldo kost, en wie er dan nog meer weg is.",
  hrLeaveTeamEmptyTitle: "Niemand heeft verlof aangevraagd",
  hrLeaveWhen: "Wanneer",
  hrLeaveWhose: "Wiens verlof",
  hrLeaveWhy: "Waarom",
  hrLeaveWithdrawn: "Ingetrokken",
  hrLeft: "Vertrokken",
  hrLoadFailed: "Dat kon niet worden geladen.",
  hrLocationHint: "Een stad, een kantoor, of “op afstand”.",
  hrManager: "Rapporteert aan",
  hrNewOpening: "Nieuwe rol",
  hrNextMonth: "De maand daarna",
  hrNoMatchBody:
    "Namen, functies, teams, e-mailadressen en telefoonnummers worden allemaal doorzocht, in willekeurige volgorde. Probeer één woord minder.",
  hrNoOpeningsBody:
    "Schrijf op welke rol u zoekt. Leg de mensen die solliciteren vast zodra ze binnenkomen, en schuif ze over het bord naarmate u ze spreekt.",
  hrNoOpeningsTitle: "Nog geen rollen opgeschreven",
  hrNobodyAway: "Er is niemand anders weg op die dagen.",
  hrNobodyAwayBody:
    "Geboekt verlof van iedereen in het bedrijf verschijnt hier, zodat u ziet wie er weg is voordat u eromheen plant. Feestdagen staan er ook bij.",
  hrNotDecided: "Vastgelegd, niet beslist",
  hrNotePlaceholder: "Wat er in het gesprek is gezegd…",
  hrNotes: "Gespreksnotities",
  hrNotesEmpty: "Er is nog niets opgeschreven.",
  hrOneDay: "1 dag",
  hrOpening: "Rol",
  hrOpeningSubtitle:
    "Een rol die is opgeschreven. Publiceren betekent dat de ronde loopt; sluiten beëindigt hem en bevriest wat de rol zei.",
  hrPerson: "Persoon",
  hrPolicyRecordedHint:
    "Deze soort wordt vastgelegd in plaats van beslist: hij is geboekt zodra u het aanvraagt.",
  hrPreviousMonth: "De maand ervoor",
  hrPublishOpening: "Publiceren",
  hrQueue: "Soort",
  hrQueueExpense: "Nota",
  hrQueueLeave: "Verlof",
  hrQueueTimesheet: "Week",
  hrRangeBackwards: "De laatste dag ligt vóór de eerste.",
  hrRetainHint:
    "Zes maanden na de sollicitatie, tenzij u iets anders opgeeft. Na deze datum mag het dossier worden gewist.",
  hrRetention: "Hoe lang we dit bewaren",
  hrRetentionExpired: "Datum verstreken",
  hrRetentionExplainer:
    "Er wordt niets automatisch gewist. Als de datum verstreken is, beslist iemand hier — en wat weg is, is weg: de gegevens, elke notitie, en het cv.",
  hrSave: "Opslaan",
  hrSaveFailed: "Die wijziging is niet opgeslagen.",
  hrScopeEveryone: "Iedereen",
  hrScopeMine: "Van mij",
  hrScopeTeam: "Mijn team",
  hrSendBack: "Terugsturen",
  hrSendBackPlaceholder: "Wat er moet worden gecorrigeerd",
  hrSendBackTitle: "Dit terugsturen?",
  hrShowBooked: "Geboekt",
  hrShowEverything: "Alles",
  hrShowInChart: "Waar ze zitten",
  hrShowWaiting: "Wacht op een beslissing",
  hrSince: "Hier sinds",
  hrSourceHint:
    "Een vacaturesite, een aanbeveling, een bureau — hoe de sollicitatie u ook heeft bereikt.",
  hrStage: "Fase",
  hrStageApplied: "Gesolliciteerd",
  hrStageHired: "Aangenomen",
  hrStageInterview: "Gesprek",
  hrStageOffer: "Aanbod",
  hrStageRejected: "Niet verder",
  hrStageReviewing: "In beoordeling",
  hrStageWithdrawn: "Teruggetrokken",
  hrStatusClosed: "Gesloten",
  hrStatusDraft: "Concept",
  hrStatusOpen: "Open",
  hrTabApprovals: "Goedkeuringen",
  hrTabAway: "Wie is weg",
  hrTabDirectory: "Adresboek",
  hrTabHiring: "Werving",
  hrTabTemplates: "Briefsjablonen",
  hrTemplatesTitle: "Briefsjablonen",
  hrTemplatesIntro: "Schrijf goedgekeurde tekst één keer en laat HR daarna een persoonlijk concept maken zonder die opnieuw te typen.",
  hrTemplatesLoadFailed: "De briefsjablonen konden niet worden geladen.",
  hrTemplatesEmpty: "Nog geen briefsjablonen",
  hrTemplatesEmptyBody: "Maak de tekst die uw bedrijf wil versturen. Vanuit dit scherm wordt niets verzonden.",
  hrTemplateNew: "Nieuw sjabloon",
  hrTemplateCreateTitle: "Briefsjabloon maken",
  hrTemplateEditTitle: "Briefsjabloon bewerken",
  hrTemplateEditorIntro: "Plaatshouders worden pas ingevuld wanneer HR een concept voor een specifieke collega maakt.",
  hrTemplateName: "Naam van sjabloon",
  hrTemplateSubject: "Onderwerp van e-mail",
  hrTemplateBody: "Tekst van brief",
  hrTemplateBodyHint: "Gebruik de goedgekeurde plaatshouders hieronder. Onbekende plaatshouders worden geweigerd.",
  hrTemplateInsertField: "Plaatshouder invoegen",
  hrTemplateSave: "Sjabloon opslaan",
  hrTemplateSaveFailed: "Het briefsjabloon is niet opgeslagen.",
  hrTemplateDelete: "Sjabloon verwijderen",
  hrTemplateDeleteTitle: (name: string) => `${name} verwijderen?`,
  hrTemplateDeleteBody: "Bestaande conceptbrieven blijven ongewijzigd. Dit sjabloon is niet meer beschikbaar voor nieuwe brieven.",
  hrTemplateDeleteFailed: "Het briefsjabloon is niet verwijderd.",
  hrTemplateFields: (count: number) => count === 1 ? "1 plaatshouder" : `${count} plaatshouders`,
  hrTabLeave: "Mijn verlof",
  hrThisMonth: "Deze maand",
  hrUnpaid: "Onbetaald",
  hrViewOrg: "Organigram",
  hrViewPeople: "Mensen",
  hrWaitingSince: "Ingediend",
  hrWhat: "Wacht op u",
  hrWhyHint:
    "Optioneel. Alleen wie hierover beslist leest het, en het wordt nooit gelogd.",
  hrWithdraw: "Intrekken",
  hrYou: "U",
  hrAppliedOn: (moment: string) => `Gesolliciteerd ${moment}`,
  hrApprovalsQueueFailed: (kinds: string) =>
    `Een deel van wat er wacht kon niet worden gelezen (${kinds}), dus deze lijst is niet volledig. Al het overige wordt getoond.`,
  hrAwayThisMonth: (count: number) =>
    count === 1 ? "1 persoon weg deze maand" : `${count} mensen weg deze maand`,
  hrBalanceAsOf: (day: string) =>
    `Berekend op ${day}, op uw eigen werkpatroon.`,
  hrCloseConfirm: (title: string) =>
    `De ronde voor ${title} sluiten? De mensen die hebben gesolliciteerd blijven staan als vastlegging van wat er is gebeurd, en de ronde kan niet worden heropend.`,
  hrClosedOn: (day: string) => `gesloten ${day}`,
  hrCountOf: (kind: string, count: number) => `${kind}: ${count}`,
  hrCvOnFile: (fileName: string) =>
    fileName === ""
      ? "Er is een cv aanwezig. Een bestand kiezen vervangt het; het vervangen cv gaat naar de HR-prullenbak."
      : `${fileName} is aanwezig. Een bestand kiezen vervangt het; het vervangen cv gaat naar de HR-prullenbak.`,
  hrDayAway: (day: string, count: number) =>
    count === 0 ? `${day}: niemand weg` : `${day}: ${count} weg`,
  hrDaysOf: (days: string) => `${days} dagen`,
  hrEraseConfirm: (name: string) =>
    `Alles over ${name} wissen? Hun gegevens, elke notitie die over hen is geschreven en hun cv worden definitief verwijderd. Dit kan niet ongedaan worden gemaakt.`,
  hrFactOf: (label: string, value: string) => `${label} ${value}`,
  hrHireKnown: (name: string) =>
    `${name} staat al in de directory met dit adres. Dit dossier toevoegen zou een tweede collega met hetzelfde e-mailadres opleveren.`,
  hrHireKnownLeft: (name: string) =>
    `${name} had dit adres en is vertrokken. Als dit dezelfde persoon is die terugkomt, is ze hier toevoegen juist — hun oude dossier blijft zoals het was.`,
  hrLeaveBetween: (from: string, to: string) => `${from} – ${to}`,
  hrLeaveOf: (policy: string, from: string, to: string) =>
    from === to ? `${policy}, ${from}` : `${policy}, ${from} – ${to}`,
  hrMoreAway: (count: number) => `+${count} meer`,
  hrNoMatchTitle: (query: string) => `Niemand komt overeen met “${query}”`,
  hrNobodyAwayTitle: (month: string) => `Niemand is weg in ${month}`,
  hrOpenedOn: (day: string) => `open sinds ${day}`,
  hrPeopleCount: (count: number) =>
    count === 1 ? "1 persoon" : `${count} mensen`,
  hrReportsCount: (count: number) =>
    count === 1 ? "1 rapporterende" : `${count} rapporterenden`,
  hrRetentionUntil: (day: string) => `Bewaard tot ${day}.`,
  hrSendBackBody: (person: string) =>
    `${person} ziet dit opnieuw, bewerkbaar, met wat u hier schrijft. Zeg wat er moet worden gecorrigeerd.`,
  hrShowingOf: (shown: number, total: number) => `${shown} van ${total}`,
  hrWaitingCount: (count: number) =>
    count === 1 ? "1 wachtend" : `${count} wachtend`,
  hrWorkingDays: (days: number) => (days === 1 ? "1 dag" : `${days} dagen`),
  userApps: "Apps",
  userAppsHint: "Alleen de aangevinkte apps staan in de navigatie van deze persoon, en de server weigert de rest — dit verbergt niet alleen, het sluit ook af. E-mail en Start kunnen niet worden uitgezet. Een app aanvinken geeft nog geen toegang tot alles erin: Finance vraagt nog steeds om de boekhoudersrol en een Space nog steeds om lidmaatschap.",
  userAppsSelfHint: "Dit is uw eigen account. Beheerders worden nooit buitengesloten, dus deze schakelaars veranderen niets aan wat u kunt openen — ze worden bewaard voor als dit account ooit geen beheerder meer is.",
  accessModuleOff: "Deze app is uitgezet voor uw account.",
  accessModuleOffHint: "Een beheerder van de werkruimte kan hem weer aanzetten.",
  accessBackHome: "Terug naar Start",
  userInvite: "Uitnodiging maken",
  userInviteReady: "Instellink",
  userInviteCopy: "Kopiëren",
  userInviteCopied: "Gekopieerd",
  userInviteHint: "Stuur deze link naar uw collega. Hij werkt één keer, verloopt na zeven dagen, en zij kiezen zelf hun wachtwoord en herstel-adres — u komt het nooit te weten. Deze link wordt maar één keer getoond.",
  inviteTitle: "Stel uw account in",
  inviteUnavailable: "Deze uitnodiging werkt niet meer",
  inviteAskAdmin: "Vraag de beheerder van uw werkruimte om een nieuwe.",
  inviteLoadFailed: "Deze uitnodiging is verlopen of is al gebruikt.",
  inviteFailed: "Dat kon niet worden opgeslagen. Probeer het opnieuw.",
  invitePassword: "Kies een wachtwoord",
  invitePasswordHint: "Minstens 8 tekens. Alleen u kent het.",
  inviteRecovery: "Herstel-adres",
  inviteRecoveryPlaceholder: "u@ergens-anders.nl",
  inviteRecoveryHint: "Een adres dat u ergens anders kunt lezen — niet dit nieuwe. Als u ooit uw wachtwoord vergeet, is dit de enige manier om weer binnen te komen zonder het aan een beheerder te vragen.",
  inviteSubmit: "Account instellen",
  inviteWorking: "Bezig met instellen…",
  inviteDoneTitle: "Klaar",
  inviteGoToSignIn: "Naar het aanmelden",
  inviteFor: (email: string): string => `Voor ${email}`,
  inviteDoneBody: (email: string): string =>
    `U kunt zich nu aanmelden als ${email} met het wachtwoord dat u zojuist hebt gekozen.`,

  // De adressen waarop een website antwoordt (S2.15c3). Elke prijs staat er
  // twee keer — wat het vandaag kost en wat het elk jaar daarna kost — omdat
  // de verlenging de helft is die een lokprijs verzwijgt.
  sitesDomains: "Domeinen",
  sitesDomainsLoading: "Domeinen laden…",
  sitesDomainsLoadFailed:
    "De domeinen van deze website konden niet worden geladen. Controleer uw verbinding en probeer het opnieuw.",
  sitesDomainAloAddress: "Deze website is altijd bereikbaar op",
  sitesDomainOwned: "Een domein dat u al hebt",
  sitesDomainOwnedHint:
    "Voeg het domein toe, publiceer het getoonde record bij uw DNS-provider en klik daarna op Controleren. Voor uw bezoekers verandert er niets tot het geverifieerd is.",
  sitesDomainAddress: "Domein",
  sitesDomainPlaceholder: "voorbeeld.nl",
  sitesDomainAdd: "Domein toevoegen",
  sitesDomainAddFailed: "Dat domein kon niet worden toegevoegd.",
  sitesDomainNoneBody:
    "Er is nog geen eigen domein gekoppeld. Voeg er een toe dat u al hebt, of koop er hieronder een, en deze website antwoordt daar ook.",
  sitesDomainStatusPending: "Wacht op het record",
  sitesDomainStatusVerified: "Geverifieerd",
  sitesDomainStatusLive: "In gebruik",
  sitesDomainCheck: "Controleren",
  sitesDomainVerifyFailed: "Het domein kon niet worden gecontroleerd.",
  sitesDomainNotYet:
    "Het record is nog niet zichtbaar. DNS-wijzigingen doen er een paar minuten over: laat het record staan en controleer straks opnieuw.",
  sitesDomainVerifiedNow: (domain: string): string =>
    `${domain} is geverifieerd. Deze website antwoordt daar nu.`,
  sitesDomainRecordTitle: "Publiceer dit record bij uw DNS-provider",
  sitesDomainRecordName: "Naam",
  sitesDomainRecordType: "Type",
  sitesDomainRecordValue: "Waarde",
  sitesDomainRecordHint:
    "Laat het record staan tot de controle slaagt. Sommige DNS-providers zetten het domein zelf achter de naam: doet die van u dat, laat het dan weg.",
  sitesDomainPointHint: (host: string): string =>
    `Laatste stap bij uw DNS-provider: wijs het domein met een CNAME naar ${host}. Voor een domein zonder subdomein hebt u het ALIAS- of ANAME-record van uw provider nodig.`,
  sitesDomainCopy: "Kopiëren",
  sitesDomainCopied: "Gekopieerd",
  sitesDomainRemove: "Verwijderen",
  sitesDomainRemoveConfirm: "Ja, verwijderen",
  sitesDomainRemoveHint:
    "alo antwoordt niet langer op dit domein. Het domein zelf blijft van u: bij de registry geeft u niets op.",
  sitesDomainRemoveFailed: "Dat domein kon niet worden verwijderd.",
  sitesDomainBuy: "Een domein kopen",
  sitesDomainBuyHint:
    "Zoek een naam. U ziet wat die dit jaar kost en wat elk jaar daarna kost voordat er iets gekocht wordt.",
  sitesDomainSearchLabel: "De naam die u wilt",
  sitesDomainSearchPlaceholder: "acme",
  sitesDomainSearching: "Bezig met zoeken…",
  sitesDomainSearchInvite: "Typ een naam om te zien welke extensies vrij zijn.",
  sitesDomainSearchFailed: "Die naam kon niet worden gecontroleerd.",
  sitesDomainCatalogFailed: "De domeinprijzen konden niet worden geladen.",
  sitesDomainUnconfiguredTitle: "Domeinen kopen staat hier niet aan",
  sitesDomainUnconfiguredBody:
    "Deze werkomgeving kan geen domeinnamen registreren. U kunt wel een domein koppelen dat u al hebt.",
  sitesDomainNotBuyable:
    "Deze werkomgeving kan prijzen tonen maar nog geen domein registreren, omdat er geen naamservers zijn ingesteld.",
  sitesDomainTestRegistrar: (name: string): string =>
    `${name} is een testregistrar: er wordt niets in rekening gebracht en er wordt geen echte naam geregistreerd.`,
  sitesDomainRegistrarLine: (name: string, country: string): string =>
    `Domeinen worden geregistreerd via ${name} (${country}). Prijzen zijn exclusief btw.`,
  sitesDomainAvailable: "Vrij",
  sitesDomainTaken: "Al geregistreerd",
  sitesDomainBlocked: "Niet te koop",
  sitesDomainUnsupportedEnding: "alo verkoopt deze extensie niet",
  sitesDomainPremium: "Premiumnaam",
  sitesDomainPremiumHint:
    "De registry rekent voor deze naam meer dan de gebruikelijke prijs van de extensie. De verlengprijs is de getoonde prijs, niet de gewone.",
  sitesDomainPriceLine: (today: string, renewal: string): string =>
    `${today} vandaag, daarna ${renewal} per jaar`,
  sitesDomainChoose: "Dit domein kopen",
  sitesDomainPurchaseTitle: (domain: string): string => `${domain} kopen`,
  sitesDomainPurchaseSubtitle:
    "Op wiens naam het domein komt te staan, en voor hoe lang. De prijs keurt u in de volgende stap goed; daarvoor wordt er niets in rekening gebracht.",
  sitesDomainYears: "Betaald voor",
  sitesDomainYearsHint:
    "Hoeveel jaar de eerste betaling dekt. Daarna gaat het per jaar.",
  sitesDomainYearsOption: (years: number): string =>
    years === 1 ? "1 jaar" : `${years} jaar`,
  sitesDomainAutoRenew: "Dit domein automatisch verlengen",
  sitesDomainAutoRenewHint:
    "Een domein dat niet verlengd wordt, bent u kwijt, en iedereen kan het dan overnemen. Zet dit alleen uit als u het zelf gaat verlengen.",
  sitesDomainAutoRenewOn: "Het wordt elk jaar automatisch verlengd.",
  sitesDomainAutoRenewOff:
    "Het wordt niet automatisch verlengd: u moet het zelf verlengen voordat het verloopt, anders bent u het kwijt.",
  sitesDomainRegistrant: "Op naam van",
  sitesDomainRegistrantHint:
    "De registry eist een echte persoon of onderneming die bereikbaar is. Dit gaat naar de registry en komt nooit op uw website te staan.",
  sitesDomainRegistrantName: "Volledige naam",
  sitesDomainRegistrantOrganisation: "Bedrijf (leeg laten als dat er niet is)",
  sitesDomainRegistrantEmail: "E-mailadres",
  sitesDomainRegistrantEmailHint:
    "De registry schrijft hierheen over verloop en verificatie. Een adres dat niemand leest, kost u het domein.",
  sitesDomainRegistrantStreet: "Straat en huisnummer",
  sitesDomainRegistrantPostalCode: "Postcode",
  sitesDomainRegistrantCity: "Plaats",
  sitesDomainRegistrantCountry: "Land",
  sitesDomainRegistrantCountryHint:
    "De landcode van twee letters, bijvoorbeeld nl of be.",
  sitesDomainRegistrantPhone: "Telefoon",
  sitesDomainRegistrantPhoneHint:
    "In internationale vorm, bijvoorbeeld +31201234567.",
  sitesDomainRequirementEea:
    "Deze extensie wordt alleen verkocht aan een houder binnen de Europese Economische Ruimte.",
  sitesDomainRequirementCountry: (country: string): string =>
    `Deze extensie wordt alleen verkocht aan een houder in het land ${country}.`,
  sitesDomainSeePrice: "De prijs bekijken",
  sitesDomainQuoteFailed: "Voor dat domein kon geen prijs worden opgevraagd.",
  sitesDomainApproveTitle: "Deze prijs goedkeuren",
  sitesDomainApproveSubtitle: (domain: string): string =>
    `Wat ${domain} kost, volledig, voordat er iets in rekening wordt gebracht.`,
  sitesDomainQuoteName: "Domein",
  sitesDomainQuoteTerm: "Betaald voor",
  sitesDomainQuoteToday: "Vandaag",
  sitesDomainQuoteRenewal: "Elk jaar daarna",
  sitesDomainApproveAction: (price: string): string => `${price} goedkeuren`,
  sitesDomainApproveHint:
    "Met goedkeuren legt u vast dat u akkoord gaat met precies deze bedragen. Verandert de prijs voordat er betaald is, dan vraagt alo het u opnieuw in plaats van een ander bedrag in rekening te brengen.",
  sitesDomainApproveFailed: "Die prijs kon niet worden goedgekeurd.",
  sitesDomainPurchases: "Hier gekochte domeinen",
  sitesDomainPurchasesHint:
    "Elk domein waarvan de aankoop voor deze website is begonnen, en hoe ver die is.",
  sitesDomainPurchasesNone:
    "Voor deze website is nog geen domein gekocht.",
  sitesDomainPurchasesLoadFailed:
    "De domeinaankopen konden niet worden geladen.",
  sitesDomainRefresh: "Vernieuwen",
  sitesDomainTermPrice: (price: string, years: number): string =>
    years === 1
      ? `${price} voor het eerste jaar`
      : `${price} voor de eerste ${years} jaar`,
  sitesDomainRenewalLine: (price: string): string => `daarna ${price} per jaar`,
  sitesDomainApprovedOn: (when: string): string =>
    `Prijs goedgekeurd op ${when}.`,
  sitesDomainAttempts: (attempts: number): string =>
    `Registratiepoging ${attempts}; alo blijft het proberen.`,
  sitesDomainCancel: "Afbestellen",
  sitesDomainCancelConfirm: "Ja, afbestellen",
  sitesDomainCancelFailed: "Die aankoop kon niet worden afbesteld.",
  sitesDomainStateQuoted: "Wacht op uw goedkeuring",
  sitesDomainStateApproved: "Goedgekeurd",
  sitesDomainStateAwaitingPayment: "Wacht op betaling",
  sitesDomainStatePaid: "Betaald",
  sitesDomainStateRegistering: "Wordt geregistreerd",
  sitesDomainStateRegistered: "Geregistreerd",
  sitesDomainStateConfigured: "In gebruik",
  sitesDomainStateFailed: "Niet afgerond",
  sitesDomainStateCancelled: "Afbesteld",
  sitesDomainStepQuoted:
    "Er is niets in rekening gebracht. Keurt u de prijs goed, dan gaat de aankoop door naar de betaling.",
  sitesDomainStepApproved:
    "U hebt deze prijs goedgekeurd. Daarna volgt de betaling: zodra die binnen is, registreert alo het domein en koppelt het dat zelf aan deze website.",
  sitesDomainStepAwaitingPayment:
    "Wacht tot de betaling binnen is. De registratie begint vanzelf zodra dat gebeurt.",
  sitesDomainStepPaid: "Betaald. De registratie begint binnen een minuut.",
  sitesDomainStepRegistering: "De registrar registreert de naam nu.",
  sitesDomainStepRegistered: (domain: string): string =>
    `${domain} staat op uw naam. Wordt gekoppeld aan deze website.`,
  sitesDomainStepConfigured: (domain: string): string =>
    `${domain} is geregistreerd en bedient deze website.`,
  sitesDomainStepFailed:
    "Deze aankoop kon niet worden afgerond. Er wordt hiervoor niets meer in rekening gebracht.",
  sitesDomainStepCancelled: "Afbesteld. Er is niets in rekening gebracht.",
  sitesDomainOwnerOnly:
    "Alleen de eigenaar van deze website kan domeinnamen kopen of beheren. U kunt de website zelf gewoon bewerken en publiceren.",
};
