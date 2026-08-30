// The English source catalog — the shape every locale is typed against
// (CLAUDE.md: strings externalized from day one —
// hardcoded English in components is a bug in a European product). This module
// is the translation catalog's source; the extraction/locale tooling lands
// with the i18n pass. Components read strings from here via `strings.*` — never
// inline English.
export const en = {
  // brand
  appName: "alo",
  tagline: "The sovereign, AI-native workspace for Europe.",

  // modules (rail labels + titles)
  moduleHome: "Home",
  moduleMail: "Mail",
  moduleAgenda: "Agenda",
  moduleChat: "Chat",
  moduleMeet: "Meet",
  moduleDrive: "Drive",
  moduleDocs: "Docs",
  moduleBilling: "Billing",
  billingWorkspacePurpose:
    "Customers, quotes, invoices and payments in one financial workspace.",

  // Home dashboard
  homeGreetingMorning: "Good morning",
  homeGreetingAfternoon: "Good afternoon",
  homeGreetingEvening: "Good evening",
  homeWelcome: "Welcome to alo workplace",
  homeStatUnreadEmails: "Unread emails",
  homeStatEvents: "Upcoming events",
  homeStatMessages: "Unread messages",
  homeStatFiles: "Documents",
  homeStatTasks: "Tasks due today",
  homeSubtitle: "Here's what's happening today.",
  homeToolsTitle: "Your tools",
  homeToolsSubtitle: "The apps you use most, ready when you need them.",
  homeSearchPlaceholder: "Search mail, events, tasks…",
  homeNotifications: "Notifications",
  homeTodaysCalendar: "Today's calendar",
  homeViewFullCalendar: "View full calendar",
  homeNoEventsToday: "Nothing on your calendar today.",
  homeMyTasks: "My tasks",
  homeViewAllTasks: "View all tasks",
  homeNoTasks: "Nothing due. You're all caught up.",
  homeTaskOverdue: "Overdue",
  homeTaskToday: "Today",
  agendaUntitledEvent: "Untitled event",
  homeGoToMail: "Go to Mail",
  homeViewTasks: "View Tasks",
  homeViewCalendar: "View Calendar",
  homeGoToTasks: "Open Tasks",
  homeNewTask: "New task",
  homeViewAgenda: "View Agenda",
  homeOpenChat: "Open Chat",
  homeOpenDrive: "Open Drive",
  homeComingSoonShort: "Coming soon",
  homeRecent: "Recent",
  homeStarred: "Starred",
  homeUnread: "Unread",
  homeViewAll: "View all",
  homeNoRecent: "Nothing here yet.",
  homeQuickActions: "Quick actions",
  homeCompose: "Compose",
  homeCreateEvent: "Create event",
  homeStartChat: "Start chat",
  homeUploadFile: "Upload file",
  homeCreateDoc: "Create document",
  homeToday: "Today",
  homeAgendaComingSoon: "Your agenda will live here once the calendar lands.",
  homeAskTitle: "Ask alo anything",
  homeAskBody: "Your AI assistant for everything at work.",
  homeAskCta: "Ask alo",
  homeAskPlaceholder: "Ask me anything…",
  homeAskUnavailable: "alo is unavailable right now. Please try again shortly.",
  homeMailClearTitle: "You're all caught up!",
  homeCalendarClearTitle: "No events today",
  homeTasksClearTitle: "All clear!",
  moduleAi: "Ask AI",

  // shell
  newButton: "New",
  appLauncher: "Apps",
  appLauncherAutoHint: "The apps you use most, kept up to date automatically",
  appLauncherFavorites: "Your favorites",
  appLauncherAll: "All apps",
  appLauncherMore: "More apps",
  appLauncherEdit: "Edit favorites",
  appLauncherDone: "Done",
  appLauncherCancel: "Cancel",
  appLauncherDragHint: "Drag and drop your six favorite apps",
  appLauncherAddFavorite: "Add to favorites",
  appLauncherRemoveFavorite: "Remove from favorites",
  userMenu: "Account",
  language: "Language",
  signOut: "Sign out",

  // contacts (address book)
  contactsTitle: "Contacts",
  contactsOpen: "Contacts",
  contactsSearchPlaceholder: "Search contacts…",
  contactsEmpty: "No contacts yet. Add your first one.",
  contactsSearchEmpty: "No contacts match your search.",
  contactsLoadError: "Couldn't load your contacts.",
  contactsNew: "New contact",
  contactEdit: "Edit contact",
  contactFirstName: "First name",
  contactLastName: "Last name",
  contactDisplayName: "Display name",
  contactEmail: "Email",
  contactPhone: "Phone",
  contactOrganization: "Organization",
  contactJobTitle: "Job title",
  contactNotes: "Notes",
  contactAddEmail: "Add email",
  contactAddPhone: "Add phone",
  // Named after the row: a contact with two emails and two phones otherwise
  // gives a screen reader four identical commands and four combo boxes
  // announced as their own current value.
  contactRemoveFieldNamed: (value: string) => `Remove ${value}`,
  contactKindLabel: (value: string) => `Kind of ${value}`,
  contactKindWork: "Work",
  contactKindHome: "Home",
  contactKindMobile: "Mobile",
  contactKindOther: "Other",
  contactSave: "Save",
  contactCancel: "Cancel",
  contactDelete: "Delete",
  contactDeleteConfirm: (name: string) =>
    `Delete ${name}? This cannot be undone.`,
  contactNeedsName: "Add a name or at least one email.",
  contactSaveError: "Couldn't save this contact.",
  contactDeleteError: "Couldn't delete this contact.",
  contactNoEmail: "No email",
  contactsImport: "Import",
  contactsExport: "Export",
  contactsImporting: "Importing…",
  contactsImported: (n: number, skipped: number) =>
    skipped > 0
      ? `Imported ${n} contact${n === 1 ? "" : "s"} (${skipped} skipped).`
      : `Imported ${n} contact${n === 1 ? "" : "s"}.`,
  contactsImportError: "Couldn't import that file. Is it a .vcf export?",
  contactsExportError: "Couldn't export your contacts.",
  contactsExportEmpty: "You have no contacts to export yet.",

  // import mail (IMAP wizard)
  importOpen: "Import mail",
  importTitle: "Import mail from another account",
  importIntro:
    "Bring your recent mail from Gmail, Outlook, or any IMAP account into your inbox.",
  importProvider: "Where is your mail?",
  importProviderGmail: "Gmail",
  importProviderOutlook: "Outlook",
  importProviderOther: "Other (IMAP)",
  importServer: "Mail server",
  importPort: "Port",
  importEmail: "Email address",
  importPassword: "Password",
  importAppPasswordHint:
    "For Gmail and Outlook you'll need an app password, not your normal password.",
  importStart: "Start import",
  importRunning: "Importing your mail — this can take a minute…",
  importDone: (imported: number, skipped: number) =>
    skipped > 0
      ? `Imported ${imported} message${imported === 1 ? "" : "s"} (${skipped} already here).`
      : `Imported ${imported} message${imported === 1 ? "" : "s"}.`,
  importNeedsFields: "Enter the server, your email, and your password.",
  importClose: "Close",
  signedInAs: "Signed in as",
  comingSoonTitle: "Coming soon",
  comingSoonBody:
    "This part of your workspace is on the way. Mail is ready now.",

  // auth — brand panel
  brandHeadline: "Your workspace.\nYour servers.\nYour rules.",
  brandSubtitle:
    "Mail, calendar, chat, and files — sovereign, AI-native, and hosted in Europe.",
  brandEuBadge: "Hosted on your infrastructure · EU",
  // auth — brand panel, standalone mail product (alomails)
  brandHeadlineMail: "Your mail.\nYour privacy.\nYour rules.",
  brandSubtitleMail:
    "Private, AI-native email — sovereign and hosted in Europe.",
  brandEuBadgeMail: "Sovereign email · Hosted in Europe",
  // auth — brand panel, standalone Drive product (alodrives)
  brandHeadlineDrive: "Your files.\nYour folders.\nYour rules.",
  brandSubtitleDrive:
    "Files, folders, and documents in one place — shared by where they live, and always yours.",
  brandEuBadgeDrive: "Your files, no lock-in",

  // auth — sign in
  signInHeading: "Sign in",
  signInSubtitle: "Welcome back. Enter your credentials to continue.",
  emailLabel: "Email",
  emailPlaceholder: "you@yourdomain.com",
  emailPlaceholderMail: "you@alomails.com",
  passwordLabel: "Password",
  showPassword: "Show password",
  hidePassword: "Hide password",
  rememberMe: "Remember me",
  forgotPassword: "Forgot password?",
  forgotPasswordNote: "To reset your password, contact your administrator.",
  signInButton: "Sign in",
  signingIn: "Signing in…",
  orDivider: "or",
  signInWithSso: "Sign in with SSO",
  ssoComingSoon: "Single sign-on is coming soon.",

  // auth — two-factor
  twoFactorTitle: "Two-factor authentication",
  twoFactorSubtitle: "Enter the 6-digit code from your authenticator app",
  twoFactorRecoverySubtitle: "Enter one of your recovery codes",
  twoFactorCodeLabel: "Authentication code",
  recoveryCodeLabel: "Recovery code",
  recoveryPlaceholder: "xxxx-xxxx",
  verify: "Verify",
  verifying: "Verifying…",
  useRecoveryCode: "Use a recovery code instead",
  useAuthenticator: "Use your authenticator app instead",
  backToSignIn: "Back to sign in",

  // auth — errors
  errorBadCredentials: "That email or password is not right. Please try again.",
  errorSecondFactor: "Enter your authentication code to continue.",
  errorBadOtp: "That code is not right. Please try again.",
  errorRateLimited: "Too many attempts. Please wait a moment and try again.",
  errorGeneric: "Something went wrong signing in. Please try again.",
  errorNetwork: "Cannot reach the server. Check your connection and try again.",
  signingOut: "Signing out…",

  // signup — personal accounts (ADR 0018)
  signupHeading: "Create your personal alo address",
  signupSubtitle: "Private, sovereign email — no ads, no tracking, ever.",
  signupAddressLabel: "Choose your address",
  signupPickPlaceholder: "yourname",
  signupRecoveryLabel: "Your current email",
  signupRecoveryHint:
    "We'll send a verification code here — it also becomes your account-recovery address.",
  signupSendCode: "Send verification code",
  signupSending: "Sending…",
  signupChecking: "Checking…",
  signupAvailable: "That address is available",
  signupTaken: "That address is already taken",
  signupReserved: "That address is reserved",
  signupInvalid: "Use 3–64 letters, numbers, dots, or dashes",
  signupVerifyHeading: "Enter your code",
  signupVerifySubtitle: (recovery: string) =>
    `We sent a 6-digit code to ${recovery}. It expires in 10 minutes.`,
  signupCodeLabel: "Verification code",
  signupPasswordLabel: "Choose a password",
  signupPasswordHint: "At least 8 characters.",
  signupCreate: "Create account",
  signupCreating: "Creating your account…",
  signupResend: "Resend code",
  signupVerifyError: "That code is incorrect or has expired. Please try again.",
  signupBeginError: "We couldn't send the code. Please try again.",
  signupDoneHeading: "You're all set",
  signupDoneBody: (email: string) =>
    `${email} is ready. Sign in with your new address and password.`,
  signupGoToLogin: "Go to sign in",
  signupUnavailable: "Personal signups aren't open right now.",
  signupHaveAccount: "Already have an account?",
  signupBackToLogin: "Sign in",
  signupCreateLink: "Create a personal account",

  // auth — password reset
  resetHeading: "Reset your password",
  resetSubtitle:
    "Enter your alo address — we'll email a reset code to your recovery mailbox.",
  resetAddressLabel: "Your alo address",
  resetSendCode: "Send reset code",
  resetSending: "Sending…",
  resetVerifyHeading: "Enter the code",
  resetVerifySubtitle: (address: string) =>
    `If ${address} has an alo account, a reset code is on its way to its recovery mailbox. Enter it below with a new password.`,
  resetNewPasswordLabel: "New password",
  resetSubmit: "Set new password",
  resetSubmitting: "Saving…",
  resetDoneHeading: "Password updated",
  resetDoneBody: "You can now sign in with your new password.",
  resetRequestError: "We couldn't start the reset. Please try again.",
  resetVerifyError: "That didn't work — check the code and try again.",

  // agenda (calendar)
  agendaNewEvent: "New event",
  agendaCalendars: "Calendars",
  agendaMyCalendars: "My calendars",
  agendaOtherCalendars: "Other calendars",
  agendaDay: "Day",
  agendaAgenda: "Agenda",
  agendaTomorrow: "Tomorrow",
  agendaUpcoming: "Upcoming",
  agendaNothingUpcoming: "Nothing upcoming.",
  agendaEventCount: (n: number) => (n === 1 ? "1 event" : `${n} events`),
  agendaCalendar: "Calendar",
  agendaNewCalendar: "New calendar",
  agendaNewCalendarPrompt: "Name for the new calendar",
  agendaDeleteCalendar: "Delete calendar",
  agendaToday: "Today",
  agendaPrev: "Previous",
  agendaToolbarLabel: "Calendar",
  agendaViewLabel: "View",
  agendaNext: "Next",
  agendaMonth: "Month",
  agendaWeek: "Week",
  agendaAllDay: "All day",
  agendaAway: "Away",
  agendaAwayTitle: (names: string) => `Away: ${names}`,
  agendaEventTitle: "Add a title",
  agendaEventStart: "Starts",
  agendaEventEnd: "Ends",
  agendaEventLocation: "Location",
  rsvpFrom: "From",
  rsvpAccept: "Accept",
  rsvpMaybe: "Maybe",
  rsvpDecline: "Decline",
  rsvpAccepted: "You accepted this invitation.",
  rsvpDeclined: "You declined this invitation.",
  rsvpTentative: "You responded Maybe.",
  replyResponded: "responded",
  replyFrom: (who: string, verb: string) => `${who} ${verb}`,
  replyApplied: "Updated on your event.",
  rsvpError: "Could not send your response — please try again.",
  cancelledTitle: "Cancelled:",
  cancelledRemoved: "Removed from your calendar.",
  cancelledAbsent: "This event wasn't on your calendar.",
  agendaEventGuests: "Guests",
  agendaGuestsPlaceholder: "name@example.com, another@example.com",
  agendaGuestsHint:
    "We'll email each guest an invitation they can accept in their own calendar.",
  agendaEventDescription: "Notes",
  agendaSave: "Save",
  agendaSaveThis: "This event",
  agendaSaveAll: "All events",
  agendaDelete: "Delete",
  agendaDeleteThis: "This event",
  agendaDeleteAll: "All events",
  agendaCancel: "Cancel",
  agendaNewEventTitle: "New event",
  agendaNewEventSubtitle: "Create a new event on your calendar",
  agendaEditEventSubtitle: "Update the details of your event",
  agendaCreateEvent: "Create event",
  agendaLocationPlaceholder: "Add a location or video call link",
  agendaDescriptionPlaceholder: "Add notes, agenda or any details…",
  agendaEditEventTitle: "Edit event",
  agendaEndBeforeStart: "The event ends before it starts.",
  agendaSaveError: "Couldn't save the event. Please try again.",
  agendaRepeat: "Repeat",
  agendaRepeatNone: "Does not repeat",
  agendaRepeatDaily: "Every day",
  agendaRepeatWeekly: "Every week",
  agendaRepeatWeekdays: "Every weekday (Mon–Fri)",
  agendaRepeatMonthly: "Every month",
  agendaRepeatYearly: "Every year",
  // tasks
  moduleTasks: "Tasks",
  taskProjects: "Projects",
  taskNewProject: "New project",
  taskNewProjectPrompt: "Name for the new project",
  taskMyPlate: "My plate",
  taskProposals: "Suggestions",
  taskBoard: "Board",
  taskList: "List",
  taskQuickAdd: "Add a task…",
  taskAdd: "Add",
  taskColReview: "Review",
  taskOverview: "Overview",
  taskOvTotal: "Total",
  taskOvCompleted: "Completed",
  taskOvProgress: "Progress",
  taskOvByAssignee: "Tasks by assignee",
  taskOvUpcoming: "Upcoming tasks",
  taskOvViewAll: "View all",
  taskOvTasksTotal: (n: number) => `${n} tasks total`,
  taskOvCompletedLabel: "Completed",
  taskSummaryTotal: (count: number) => `${count} total`,
  taskSummaryActive: (count: number) => `${count} active`,
  taskSummaryOverdue: (count: number) => `${count} overdue`,
  taskSummaryCompleted: (count: number) => `${count} completed`,
  taskOvNobody: "Unassigned",
  taskColName: "Task name",
  taskColProject: "Project",
  taskColAssignee: "Assignee",
  taskColDue: "Due date",
  taskColPriority: "Priority",
  taskAssigneeYou: "You",
  taskMarkDone: "Mark done",
  taskMarkNotDone: "Mark not done",
  taskNew: "New task",
  taskSearchPlaceholder: "Search tasks, projects…",
  taskEmptyTitle: "No tasks yet 👋",
  taskEmptyBody: "You're all set. Start by creating your first task.",
  taskCreateFirst: "Create your first task",
  taskShowProjects: "Show projects",
  taskHideProjects: "Hide projects",
  taskNewTaskPrompt: "Name for the new task",
  taskNewSubtitle: "Create a task and stay organized.",
  taskNamePlaceholder: "e.g. Design landing page",
  taskCancel: "Cancel",
  taskCreate: "Create task",
  taskCreating: "Creating…",
  taskAttachments: "Attachments",
  taskAddAttachment: "Add attachment",
  taskFilesEmpty: "No files yet. Attach one from any task.",
  taskFilesAttachTo: "Attach to task",
  taskFilesDropHint: "Drop images or files here, or use Add attachment.",
  taskFilesNeedTask: "Create a task first, then attach images and files to it.",
  taskFilesUploadError: "Couldn't attach those files. Please try again.",
  taskChooseFromDrive: "Choose from Drive",
  taskChooseFromDriveHint:
    "Attach existing files without uploading them again.",
  taskSearchDrive: "Search this folder",
  taskDriveBack: "Back to previous folder",
  taskNoDriveFiles: "No files in this folder.",
  taskAttachSelected: "Attach selected",
  taskFilesSelected: (count: number) =>
    count === 1 ? "1 file selected" : `${count} files selected`,
  taskCreateOnDate: (date: string) => `Create a task due ${date}`,
  taskLabelsTitle: "Labels",
  taskAddLabel: "Add label",
  taskNewLabelPlaceholder: "New label…",
  taskCreateLabel: "Create",
  taskFollowers: "Followers",
  taskFollow: "Follow",
  taskLeave: "Leave task",
  taskBlockedBy: "Blocked by",
  taskAddBlocker: "Add blocker",
  taskNoBlockerCandidates: "No other tasks to depend on",
  taskUploading: "Uploading…",
  taskDownload: "Download",
  taskCreateAnother: "Create another task",
  datePickerClear: "Clear",
  datePickerToday: "Today",
  taskAllTasks: "All tasks",
  taskUnassigned: "Unassigned",
  taskFilter: "Filter",
  taskSort: "Sort",
  taskGroup: "Group",
  taskOptions: "Options",
  taskSortManual: "Manual",
  taskSortDue: "Due date",
  taskSortPriority: "Priority",
  taskSortName: "Name",
  taskSortCreated: "Newest",
  taskGroupStatus: "Status",
  taskGroupProject: "Project",
  taskGroupAssignee: "Assignee",
  taskGroupPriority: "Priority",
  taskGroupNone: "None",
  taskOnlyMine: "Only my tasks",
  taskShowCompleted: "Show completed",
  taskCompactRows: "Compact rows",
  taskTimeline: "Timeline",
  taskCalendar: "Calendar",
  taskFiles: "Files",
  taskUnscheduled: "No due date",
  taskColTodo: "To do",
  taskColInProgress: "In progress",
  taskColDone: "Done",
  taskDueToday: "Today",
  taskDueTomorrow: "Tomorrow",
  taskDueYesterday: "Yesterday",
  taskPrioNone: "None",
  taskPrioLow: "Low",
  taskPrioMedium: "Medium",
  taskPrioHigh: "High",
  taskFromEmail: "From an email",
  taskFromEvent: "From an event",
  taskOpenEmail: "Open the source email",
  createTask: "Create a task",
  suggestTasks: "Suggest tasks from this email",
  taskCreatedFromMail: "Task created from this email.",
  taskSuggesting: "Reading the email for action items…",
  taskNoSuggestions: "No action items found in this email.",
  taskSuggested: (n: number) =>
    n === 1
      ? "1 suggestion added to your task inbox."
      : `${n} suggestions added to your task inbox.`,
  taskAiOff: "AI is off, so nothing could be suggested.",
  taskClose: "Close",
  taskDelete: "Delete",
  taskDetailDialog: "Task details",
  taskStatus: "Status",
  taskTimeTracking: "Time tracking",
  taskTimeTrackingHint: "Track this task directly into your timesheet.",
  taskTimerRunningOnTask: "Time is being tracked on this task.",
  taskTimerRunningElsewhere: "Another timer is already running.",
  taskSwitchTimer: "Switch timer",
  taskAssignee: "Assignee",
  taskAssigneePlaceholder: "name@example.com",
  taskDue: "Due",
  taskPriority: "Priority",
  taskDescription: "Description",
  taskDescriptionPlaceholder: "Add more detail…",
  taskSubtasks: "Subtasks",
  taskAddSubtask: "Add a subtask…",
  taskComments: "Comments",
  taskAddComment: "Write a comment…",
  taskActivity: "Activity",
  taskEmpty: "No tasks yet. Add one above.",
  taskPlateEmpty: "Nothing due. You’re all caught up.",
  taskNoProposalsTitle: "You're all caught up",
  taskNoProposals:
    "Suggestions appear here when alo finds action items in an email.",
  taskAiSuggested: "AI suggested",
  taskAccept: "Accept",
  taskReject: "Dismiss",
  taskActivityKind: (kind: string) =>
    (
      ({
        created: "created this task",
        status_changed: "moved it",
        assigned: "changed the assignee",
        due_changed: "changed the due date",
        commented: "commented",
        accepted: "accepted the suggestion",
        proposed: "was suggested by AI",
      }) as Record<string, string>
    )[kind] ?? kind,
  agendaReminder: "Reminder",
  agendaReminderNone: "No reminder",
  agendaReminderAtStart: "At time of event",
  agendaReminder5: "5 minutes before",
  agendaReminder10: "10 minutes before",
  agendaReminder15: "15 minutes before",
  agendaReminder30: "30 minutes before",
  agendaReminder60: "1 hour before",
  agendaReminder1Day: "1 day before",
  agendaRsvpAccepted: "Accepted",
  agendaRsvpDeclined: "Declined",
  agendaRsvpTentative: "Maybe",
  agendaRsvpPending: "No reply yet",
  agendaCheckAvailability: "Check availability",
  agendaAvailChecking: "Checking…",
  agendaAvailAllFree: "Everyone is free then.",
  agendaAvailBusy: (names: string) => `Busy then: ${names}`,
  agendaAvailNoGuests: "Add guests to check their availability.",
  agendaAvailError: "Couldn't check availability.",
  agendaAvailOutside: (names: string) =>
    `Outside working hours then: ${names}`,
  // Rooms and resources (the thing a meeting needs besides people)
  agendaRoom: "Room",
  agendaRoomNone: "No room",
  agendaRoomHint: "The room is invited with the meeting and held for its time.",
  agendaRoomSeats: (seats: number) => `seats ${seats}`,
  agendaRoomTaken: (name: string) => `${name} is already booked then.`,
  // Working hours (the schedule colleagues' scheduling checks against)
  agendaWorkingHours: "Working hours",
  agendaWorkingHoursHint:
    "When colleagues schedule with you, time outside these hours is flagged.",
  agendaWorkingDays: "Working days",
  agendaWorkStart: "Start",
  agendaWorkEnd: "End",
  agendaWorkZone: "Time zone",
  agendaWorkZoneMine: "My time zone",
  agendaWorkHoursOrder: "Working hours end before they start.",
  agendaWorkingHoursError: "Couldn't save your working hours. Please try again.",
  agendaWorkingHoursLoadError: "Couldn't load your working hours.",
  agendaClose: "Close",
  agendaReadOnly: "You have view-only access to this calendar.",
  // Calendar sharing
  agendaShare: "Share calendar",
  agendaShareTitle: (name: string) => `Share “${name}”`,
  agendaShareWith: "Share with",
  agendaSharePerson: "A person",
  agendaShareGroupOption: "A group",
  agendaShareEmail: "Email address",
  agendaShareEmailPlaceholder: "name@example.com",
  agendaShareGroupPick: "Choose a group…",
  agendaShareAccess: "Access",
  agendaShareViewer: "Can view",
  agendaShareEditor: "Can edit",
  agendaShareGroup: "Group",
  agendaShareAdd: "Share",
  agendaShareRemove: "Remove",
  agendaShareRemoveFor: (name: string) => `Stop sharing with ${name}`,
  agendaShareEmpty: "Not shared with anyone yet.",
  agendaShareLoadError: "Couldn't load who this is shared with.",
  agendaShareError: "Couldn't update sharing. Please try again.",

  // mail
  mailLoading: "Loading your mail…",
  mailSearching: "Searching…",
  mailFolders: "Folders",
  flaggedView: "Flagged",
  // Flag follow-up due-date
  flagDueAdd: "Add due date",
  flagDueToday: "Today",
  flagDueTomorrow: "Tomorrow",
  flagDueNextWeek: "Next week",
  flagDuePick: "Pick a date…",
  flagDueClear: "Clear due date",
  flagDueLabel: (when: string) => `Due ${when}`,
  flagDueOverdue: (when: string) => `Overdue — was due ${when}`,
  flagDueSet: "Set a follow-up date",
  resizeFolders:
    "Resize the folders panel (drag, or arrow keys; double-click to reset)",
  resizeMessages:
    "Resize the message list (drag, or arrow keys; double-click to reset)",
  collapseFolders: "Hide folders",
  expandFolders: "Show folders",
  mailEmpty: "No messages here yet.",
  mailSearchEmpty: "No messages match your search.",
  mailSelectPrompt: "Your inbox is ready",
  mailSelectBody: "Choose a message from the list to open the conversation.",
  mailListError: "Could not load messages.",
  mailFolderError: "Could not load your folders.",
  mailRetry: "Try again",
  mailFrom: "From",
  mailTo: "To",
  mailNoSubject: "(no subject)",
  mailUnknownSender: "Unknown sender",

  // mail — sidebar
  compose: "Compose",
  mailSearchPlaceholder: "Search mail…",
  viewAsMessages: "Show as individual messages",
  viewAsConversations: "Show as conversations",

  // mail — reading pane
  conversationActions: "Conversation actions",
  reply: "Reply",
  replyAll: "Reply all",
  forward: "Forward",
  archive: "Archive",
  snooze: "Snooze",
  flag: "Flag",
  unflag: "Unflag",
  markRead: "Mark as read",
  markUnread: "Mark as unread",
  selectAll: "Select all",
  selectNone: "Clear selection",
  selectedCount: (n: number) => (n === 1 ? "1 selected" : `${n} selected`),
  snoozeUntil: "Snooze until…",
  snoozeLaterToday: "Later today",
  snoozeTomorrow: "Tomorrow",
  snoozeWeekend: "This weekend",
  snoozeNextWeek: "Next week",
  mailSnoozed: "Snoozed",
  delete: "Delete",
  dialogConfirm: "Confirm",
  dialogCancel: "Cancel",
  dialogOk: "OK",
  deletePermanently: "Delete permanently",
  moveTo: "Move to folder",
  moreActions: "More actions",
  mailMoved: "Message moved.",
  mailDeleted: "Message deleted.",
  mailActionFailed: "That didn't work — please try again.",
  endOfMessage: "End of message",
  threadMessages: "messages",
  aloSummary: "alo summary",
  summaryPending: "Summarizing this conversation…",
  smartReplies: "Suggested replies",
  quickReplyHint: "Reply all · Forward above",
  toLabel: "to",
  ccLabel: "cc",
  bccLabel: "bcc",
  recipientsNone: "—",
  senderVerified: "Verified",
  senderVerifiedTitle: "Sender authenticated — SPF, DKIM and DMARC all passed",
  replyTo: "Reply to",
  quickReplyTo: (name: string) => `Quick reply to ${name}`,
  replyToName: (name: string) => `Reply to ${name}…`,
  draftWithAi: "Draft with AI",
  attachments: "Attachments",
  attach: "Attach files",
  attachmentUploading: "Uploading…",
  attachmentDownloading: "Downloading…",
  attachmentUploadFailed: "Couldn't upload that file.",
  downloadAttachment: (name: string) => `Download ${name}`,
  attachmentFailed: "Couldn't download that attachment.",

  // mail — compose
  composeTitle: "New message",
  composeEdit: "Edit draft",
  composeEditTitle: "Edit message",
  composeReplyTitle: "Reply",
  composeForwardTitle: "Forward",
  composeForwardPrefix: "Fwd: ",
  composeForwardedIntro: "---------- Forwarded message ----------",
  composeLabelFrom: "From:",
  composeLabelDate: "Date:",
  composeLabelSubject: "Subject:",
  composeLabelTo: "To:",
  composeReplyAllTitle: "Reply all",
  composeFrom: "From",
  composeTo: "To",
  composeCc: "Cc",
  composeBcc: "Bcc",
  composeSubject: "Subject",
  composeRecipientsPlaceholder: "name@example.com, …",
  composeSubjectPlaceholder: "Subject",
  composeBodyPlaceholder: "Write your message…",
  composeSend: "Send",
  composeSending: "Sending…",
  composeDiscard: "Discard",
  composeCcToggle: "Cc",
  composeNoRecipients: "Add at least one recipient.",
  composeSendError: "Could not send your message. Please try again.",
  composeSent: "Message sent.",
  composeUndoWindow: "Sending…",
  composeUndoSend: "Undo",
  composeSendUndone: "Send undone — your message is in Drafts.",
  scheduleSend: "Schedule send",
  scheduleTomorrowMorning: "Tomorrow morning",
  scheduleTomorrowAfternoon: "Tomorrow afternoon",
  scheduleMondayMorning: "Monday morning",
  schedulePickTime: "Pick date & time",
  mailScheduled: (when: string) => `Send scheduled for ${when}.`,
  scheduleError: "Could not schedule your message. Please try again.",
  cancelSend: "Cancel send",
  sendCancelled: "Scheduled send cancelled — your message is back in Drafts.",
  contactSuggestions: "Matching contacts",
  labelColor: "Label color",
  labelColorHint: "right-click to color",
  labelColorClear: "No color",
  folderNew: "New folder",
  folderNewSub: "New subfolder",
  folderRename: "Rename",
  folderDelete: "Delete folder",
  folderNamePlaceholder: "Folder name",
  folderDeleteConfirm: (name: string) =>
    `Delete the folder "${name}"? Its messages are not deleted.`,
  folderActionFailed: "That folder change didn't work — please try again.",
  folderActions: (name: string) => `Options for the ${name} folder`,
  // Shared mailboxes / delegation
  sharedMailboxLabel: "Mailbox",
  sharedMailboxesHeading: "Shared mailboxes",
  sharedMyMailbox: "My mailbox",
  sharedReadOnly: "read-only",
  sharedNoSend:
    "You can't send from this shared mailbox — you weren't granted send access.",
  // Self-service sharing (Settings)
  settingsSharing: "Sharing",
  settingsSharingHint:
    "Let colleagues open and manage your mailbox. Grant send access to also let them send as you.",
  sharingNone: "You haven't shared your mailbox with anyone.",
  sharingEmailPlaceholder: "Colleague's email",
  sharingAdd: "Share",
  sharingAddError:
    "Couldn't share — check the email is a colleague in your organization.",
  // App-specific passwords (Settings)
  settingsAppPasswords: "App passwords",
  settingsAppPasswordsHint:
    "Passwords for mail apps that sign in the classic way (IMAP, POP3, SMTP) — like Thunderbird or the mail app on your phone. Each app gets its own password, so you can revoke one without changing your account password.",
  appPasswordNone:
    "No app passwords yet. Create one when a mail app asks for a password — especially if your account uses two-step sign-in, which classic mail apps can't carry.",
  appPasswordNamePlaceholder:
    "What is it for? e.g. Thunderbird on the desk machine",
  appPasswordCreate: "Create password",
  appPasswordCreated: (date: string) => `Created ${date}`,
  appPasswordLastUsed: (date: string) => `Last used ${date}`,
  appPasswordNeverUsed: "Never used",
  appPasswordRevokeFor: (name: string) => `Revoke ${name}`,
  appPasswordSecretFor: (name: string) => `Password for “${name}”`,
  appPasswordSecretHint:
    "Copy it into the app now — for your security it can't be shown again.",
  appPasswordCopy: "Copy password",
  appPasswordCopied: "Copied",
  appPasswordSecretDone: "Done",
  appPasswordListError: "Couldn't load your app passwords — please try again.",
  appPasswordCreateError:
    "Couldn't create the app password — give it a short name, and note an account holds at most 20.",
  appPasswordRevokeError: "Couldn't revoke it — please try again.",
  // Notifications (Web Push, Settings)
  settingsNotifications: "Notifications",
  settingsNotificationsHint:
    "Get a nudge on this device when something new arrives — even with alo closed. Each device opts in on its own, and you can switch any of them off from here.",
  pushLoadError: "Couldn't load your notification settings — please try again.",
  pushNotAvailable: "Notifications aren't switched on for this server yet.",
  pushUnsupported: "This browser can't show notifications from installed apps.",
  pushThisDevice: "Notifications on this device",
  pushOnNote: "On — you'll hear about new mail even when alo isn't open.",
  pushOffNote: "Off — this device stays quiet.",
  pushEnable: "Turn on",
  pushDisable: "Turn off",
  pushPermissionBlocked:
    "The browser is blocking notifications for this site. Allow them in the browser's site settings, then try again.",
  pushThisDeviceTag: "This device",
  pushDeviceSince: (date: string) => `Since ${date}`,
  pushDeviceRemove: (name: string) => `Stop notifications on ${name}`,
  pushPrivacyNote:
    "A notification is only a nudge — what actually arrived stays in alo until you open it.",
  pushError: "Couldn't update notifications — please try again.",
  // Admin — mailbox delegation
  userShareAccess: "Shared access",
  delegateTitle: (email: string) => `Who can access ${email}`,
  delegateIntro:
    "People you add can open and manage this mailbox. Allow sending to also let them send as this address.",
  delegatePeople: "People with access",
  delegateNone: "No one else has access yet.",
  delegateAdd: "Add person",
  delegateReadOnly: "Read-only",
  delegateManage: "Can manage",
  delegateAccessLabel: "Access level",
  delegateSendLabel: "Send permission",
  delegateSendNone: "Can't send",
  delegateSendAs: "Send as",
  delegateSendOnBehalf: "Send on behalf",
  delegateRemove: "Remove access",
  // The same two buttons on a row about one person. In a list of five
  // colleagues, "Remove access" five times says nothing about whose.
  delegateRemoveFor: (email: string) => `Remove ${email}'s access`,
  delegateFoldersFor: (email: string) => `Limit ${email} to folders`,
  delegateError: "That access change didn't work — please try again.",
  // Per-folder access (ADR 0017)
  delegateFoldersLabel: "Limit to folders",
  delegateWholeMailbox: "Whole mailbox",
  delegateLimitFolders: "Limit access to specific folders",
  delegateFoldersSave: "Save folders",
  delegateFoldersCancel: "Cancel",
  // Categories (colored message labels)
  categories: "Categories",
  categorize: "Categorize",
  categoryNew: "New category",
  categoryRename: "Rename",
  categoryDelete: "Delete category",
  categoryNamePlaceholder: "Category name",
  categoryNoneHint: "No categories yet — add one from the sidebar.",
  categoryDeleteConfirm: (name: string) =>
    `Delete the category "${name}"? It is removed from every message that has it.`,
  categoryActionFailed: "That category change didn't work — please try again.",
  categoryActions: (name: string) => `Options for the ${name} category`,
  categoryClearFilter: "Show all messages",
  // alo Transfer (large files as expiring links)
  transferLink: "link",
  transferSharedFile: "📎 Shared file",
  transferDownload: "Download",
  transferExpires: (date: string) => `link expires ${date}`,
  transferExpiryTitle: "How long large-file links stay live",
  transferExpiryOption: (days: number) =>
    days === 1 ? "1 day" : `${days} days`,
  blockSenderNamed: (email: string) => `Block ${email}`,
  senderBlocked: (email: string) =>
    `Blocked ${email} — their mail now goes to Junk.`,
  // Filters & rules
  settingsFilters: "Filters & rules",
  settingsFiltersHint:
    "Rules run on your server as mail arrives — even when you're offline. The first matching rule applies.",
  filtersLoadError: "Could not load your filters.",
  filtersSaveError: "Could not save your filters. Please try again.",
  filterAddRule: "Add a rule",
  filterNamePlaceholder: "Rule name (optional)",
  filterWhen: "When a message arrives and",
  filterDo: "Do this",
  filterMatchAll: "all match",
  filterMatchAny: "any match",
  filterOr: "or",
  filterFieldFrom: "From",
  filterFieldTo: "To",
  filterFieldCc: "Cc",
  filterFieldSubject: "Subject",
  filterOpContains: "contains",
  filterOpIs: "is exactly",
  filterValuePlaceholder: "value",
  filterAddCondition: "Add condition",
  filterRemoveCondition: "Remove condition",
  // A rule is a numbered list of conditions, and each condition is three
  // controls. Without the number, a screen reader hears "field, match, value"
  // three times over and cannot tell which row it is in.
  filterConditionField: (n: number) => `Condition ${n}: field`,
  filterConditionOp: (n: number) => `Condition ${n}: match`,
  filterConditionValue: (n: number) => `Condition ${n}: value`,
  filterRemoveConditionAt: (n: number) => `Remove condition ${n}`,
  filterRuleEnabled: (rule: string) => `Rule active: ${rule}`,
  filterFolderLabel: "Destination folder",
  filterActionFileInto: "Move to folder",
  filterActionMarkRead: "Mark as read",
  filterActionStar: "Star it",
  filterActionDelete: "Delete it",
  filterSaveRule: "Save rule",
  filterCancel: "Cancel",
  filterDelete: "Delete rule",
  filterNeedsCondition: "Add at least one condition with a value.",
  filterNeedsAction: "Choose at least one action.",
  composeWroteOn: "wrote:",
  composeReplyPrefix: "Re: ",
  composeBack: "Back",
  composeExpand: "Full screen",
  composeCollapse: "Exit full screen",
  composeMinimize: "Minimize",
  composeRestore: "Restore",
  showQuoted: "Show quoted text",
  showOriginal: "Show original",
  downloadEml: "Download .eml",
  print: "Print",
  reportSpam: "Report spam",
  notSpam: "Not spam",
  // Spam "why was this flagged" banner (reading pane, Junk folder)
  spamBannerTitle: "This message is in Spam",
  spamReasonDmarc: (domain: string) =>
    `We couldn't confirm it was really sent from ${domain} — it failed DMARC authentication, a common sign of spoofing.`,
  spamReasonDkim:
    "Its cryptographic signature (DKIM) didn't validate, so the sender couldn't be verified.",
  spamReasonSpf: (domain: string) =>
    `The server that sent it isn't authorized to send mail for ${domain} (SPF failed).`,
  spamReasonNone:
    "We didn't detect a delivery problem with this message — it may match mail that you or a filter rule marked as spam before.",
  spamBannerHint: "If this isn't spam, move it back to your Inbox.",
  spamSenderFallback: "the sender's domain",
  // One-click unsubscribe (RFC 8058)
  unsubscribe: "Unsubscribe",
  unsubscribeConfirm: (sender: string) =>
    `Unsubscribe from ${sender}? We'll ask the sender to stop emailing you.`,
  unsubscribed: "Unsubscribed — the sender was asked to stop.",
  unsubscribeFailed:
    "Couldn't unsubscribe automatically — try the link in the message.",
  unsubscribeOpened: "Opened the unsubscribe page in a new tab.",
  forwardAsAttachment: "Forward as attachment",
  blockSender: "Block sender",
  junkUnavailable: "There's no Junk folder to move this to.",
  hideQuoted: "Hide quoted text",
  formatting: "Text formatting",
  bold: "Bold",
  italic: "Italic",
  underline: "Underline",
  link: "Insert link",
  linkPrompt: "Link URL:",
  improve: "Improve",
  aiImproveFailed: "The AI couldn't rewrite that just now.",

  // account settings (signature + org footer)
  settingsOpen: "Settings",
  settingsTitle: "Mail settings",
  settingsTabGeneral: "General",
  settingsTabOrg: "Organization",
  settingsOooToggle: "Send automatic replies",
  settingsSignature: "Your signature",
  settingsSignatureHint: "Added to the bottom of messages you send…",
  settingsOrgFooter: "Organization footer",
  settingsOrgFooterHint:
    "Added to every user's outgoing mail, after their signature.",
  settingsOrgFooterPlaceholder: "e.g. company name, address, legal notice…",
  settingsOutOfOffice: "Out of office",
  settingsOutOfOfficeHint:
    "Automatically reply once to anyone who emails you while you're away.",
  settingsOooSubjectPlaceholder: "Subject (optional) — e.g. Out of office",
  settingsOooMessagePlaceholder:
    "e.g. I'm away until Monday and will reply on my return.",
  settingsOooNeedsMessage: "Add a message to turn on out-of-office.",
  settingsOooFrom: "First day away",
  settingsOooTo: "Last day away",
  settingsOooDatesHint:
    "Leave these blank to start now and reply until you turn it off.",
  settingsOooBadWindow: "The last day away can't be before the first.",
  settingsSave: "Save",
  settingsSaved: "Saved.",
  settingsSaveError: "Couldn't save your settings.",
  settingsLoadError: "Couldn't load your settings.",

  // admin console
  adminTitle: "Admin",
  adminBackToalo: "Back to alo",
  adminOpen: "Admin console",

  // admin — overview dashboard
  adminOverview: "Overview",
  adminOverviewIntro: "Your organization at a glance.",
  overviewUsers: "Users",
  overviewStorage: "Storage used",
  overviewDeliverability: "Deliverability",
  overviewDeliverOk: "All checks passing",
  overviewDeliverAttention: "Needs attention",
  overviewAi: "AI",
  overviewOn: "On",
  overviewOff: "Off",
  overviewManage: "Manage",

  // admin — domains (tenant's own; ADR 0012)
  adminDomains: "Domains",
  adminDomainsIntro:
    "Domains this organization sends and receives mail for, and their verification.",
  adminDomainsError: "Couldn't load domains.",
  adminDomainsEmpty: "No domains yet. Add one to verify it.",
  adminAddDomain: "Add domain",
  dkimPublish: "Publish this DKIM record so your mail is signed",
  dkimRotate: "Rotate DKIM",
  dkimRotateConfirm: (domain: string) =>
    `Rotate the DKIM key for ${domain}? Publish the new record; keep the old one until mail stops using it.`,
  dkimRotated: (domain: string) =>
    `New DKIM key for ${domain} — publish the updated record.`,

  // admin — audit log
  adminAudit: "Audit log",
  adminAuditIntro: "Who changed what, and when. Newest first.",
  adminAuditError: "Couldn't load the audit log.",
  adminAuditEmpty: "No administrative actions recorded yet.",
  auditBy: (actor: string) => `by ${actor}`,
  auditUnknownActor: "system",
  auditUserCreate: "Created a user",
  auditUserDelete: "Deleted a user",
  auditUserAdmin: "Changed admin rights",
  auditAliasAdd: "Added an alias",
  auditAliasRemove: "Removed an alias",
  auditGroupCreate: "Created a group",
  auditGroupDelete: "Deleted a group",
  auditGroupAddress: "Changed a list address",
  auditDomainRegister: "Registered a domain",
  auditDomainVerify: "Verified a domain",
  auditDomainDelete: "Removed a domain",
  auditTenantCreate: "Created the tenant",
  auditTenantStatus: "Changed tenant status",
  auditTenantQuota: "Changed the storage quota",

  // control plane (platform operator; ADR 0012)
  controlOpen: "Control plane",
  controlTitle: "Control plane",
  controlDeniedTitle: "Operator access required",
  controlDeniedBody:
    "The control plane is for platform operators. Your account isn't one — ask an operator if you need access.",
  controlTenants: "Tenants",
  controlTenantsIntro: "Every organization on this deployment.",
  controlTenantsError: "Couldn't load tenants.",
  controlTenantsEmpty: "No tenants yet. Create the first one.",
  controlDomains: "Domains",
  controlDomainsIntro:
    "Domains each tenant may send and receive mail for, and their verification.",
  controlDomainsError: "Couldn't load domains.",
  controlDomainsEmpty: "No domains registered yet.",
  tenantAdd: "New tenant",
  tenantName: "Organization name",
  tenantNameHint: "Acme GmbH",
  tenantAdminEmail: "First admin email",
  tenantAdminPassword: "First admin password",
  tenantAdminPasswordHint: "at least 12 characters",
  tenantCreate: "Create tenant",
  tenantInvalid:
    "A name, a valid admin email, and a 12+ character password are required.",
  tenantCreateError: "Couldn't create that tenant.",
  tenantActive: "Active",
  tenantSuspended: "Suspended",
  tenantSuspend: "Suspend",
  tenantResume: "Resume",
  tenantDelete: "Delete tenant",
  tenantDeleteConfirm: (name: string) =>
    `Permanently delete "${name}" and all of its data? This cannot be undone.`,
  tenantUsage: (n: number, size: string) =>
    `${n === 1 ? "1 user" : `${n} users`} · ${size}`,
  tenantQuota: "Quota",
  tenantQuotaPrompt: "Storage quota in GB (leave blank for unlimited):",
  tenantQuotaUnlimited: "unlimited",
  tenantQuotaOf: (size: string) => `of ${size}`,
  domainAdd: "Add domain",
  domainTenant: "Owning tenant",
  domainName: "Domain",
  domainRegister: "Register",
  domainInvalid: "Choose a tenant and enter a valid domain.",
  domainCreateError: "Couldn't register that domain.",
  domainActionError: "That didn't work. Try again.",
  domainVerified: "Verified",
  domainUnverified: "Unverified",
  domainVerify: "Verify",
  domainDelete: "Remove domain",
  domainOwnedBy: (tenant: string) => `Owned by ${tenant}`,
  domainDeleteConfirm: (domain: string) =>
    `Remove ${domain} from this deployment?`,
  domainVerifiedOk: (domain: string) => `${domain} is verified.`,
  domainVerifyPending: (domain: string) =>
    `No matching DNS TXT record found for ${domain} yet — publish it and try again.`,
  domainPublishTitle: "Publish this DNS record",
  domainPublishIntro: (domain: string) =>
    `To prove ownership of ${domain}, publish this TXT record, then click Verify on the domain.`,
  domainRecordName: "Record name",
  domainRecordType: "Type",
  domainRecordValue: "Value",
  domainPublishDone: "Done",

  adminDeniedTitle: "Admin access required",
  adminDeniedBody:
    "You don't have administrator access to this workspace. Ask an admin to grant it if you need it.",
  adminSecurity: "Security & trust",
  adminSecurityIntro:
    "How your mail domain looks to the outside world. These checks query live DNS and the MTA-STS policy each time you run them.",
  securityFor: (domain: string) => `Checks for ${domain}`,
  securityRecheck: "Run checks again",
  securityChecking: "Running live checks…",
  securityError: "Couldn't run the checks — please try again.",
  securityPass: "Pass",
  securityWarn: "Attention",
  securityFail: "Action needed",
  adminGroups: "Groups & lists",
  adminGroupsIntro:
    "Groups for shared access, and distribution lists that fan mail out to their members.",
  adminNewGroup: "New group",
  adminGroupsError: "Couldn't load groups.",
  groupName: "Group name",
  groupRename: "Rename",
  groupCreate: "Create group",
  groupListBadge: "List",
  groupMembers: "Members",
  groupMemberCount: (n: number) => (n === 1 ? "1 member" : `${n} members`),
  groupNoMembers: "No members yet.",
  groupListAddress: "List address",
  groupListAddressHint:
    "Mail sent to this address is delivered to every member. Leave blank for a plain access group.",
  groupAddressSave: "Save address",
  groupAddressClear: "Turn off list",
  groupAddMember: "Add member",
  groupDelete: "Delete group",
  groupDeleteConfirm: (name: string) =>
    `Delete the group “${name}”? Members keep their mailboxes.`,
  groupCreateError:
    "Couldn't create that group — the name may already be taken.",
  groupAddressError: "Couldn't set that address — it may already be in use.",
  groupActionError: "That didn't work — please try again.",
  groupClose: "Close",
  adminUsers: "Users & mailboxes",
  adminUsersIntro: "People in your organization and their mailboxes.",
  adminAddUser: "Add user",
  adminUsersError: "Couldn't load users.",
  userAdminBadge: "Admin",
  userManage: "Manage",
  userUsage: (n: number, size: string) =>
    `${n === 1 ? "1 message" : `${n} messages`} · ${size}`,
  userEmail: "Email",
  userPassword: "Password",
  userNewPassword: "New password",
  userPasswordHint: "At least 8 characters.",
  userCreate: "Create user",
  userInvalid: "Enter a valid email and a password of at least 8 characters.",
  userCreateError:
    "Couldn't create that user — the email may already be in use.",
  userReset: "Reset password",
  userResetDone: "Password reset.",
  userAdminRole: "Tenant admin",
  userAdminRoleFor: (email: string) => `Tenant admin access for ${email}`,
  userAdminHint: "Admins can manage users, aliases, and settings.",
  userRoles: "Roles",
  // The per-user app switches (migration 0208). The hint has to say which
  // promise is being made: "hidden" and "refused" are very different, and an
  // administrator switching something off is entitled to know which one they
  // are getting.
  // Inviting somebody instead of choosing their password (migration 0209).
  // The hint says what the admin gets *and* what they deliberately do not.
  userInvite: "Create an invitation",
  userInviteReady: "Setup link",
  userInviteCopy: "Copy",
  userInviteCopied: "Copied",
  userInviteHint:
    "Send this link to your colleague. It works once, expires after seven days, and they choose their own password and recovery address — you never learn either. This link is shown only once.",
  // The account-claiming page. The first screen of alo anybody outside the
  // admin console sees, so it explains rather than instructs.
  inviteTitle: "Set up your account",
  inviteUnavailable: "This invitation no longer works",
  inviteAskAdmin: "Ask your workspace administrator for a new one.",
  inviteLoadFailed: "This invitation has expired or has already been used.",
  inviteFailed: "That could not be saved. Try again.",
  invitePassword: "Choose a password",
  invitePasswordHint: "At least 8 characters. Only you will know it.",
  inviteRecovery: "Recovery address",
  inviteRecoveryPlaceholder: "you@somewhere-else.com",
  inviteRecoveryHint:
    "An address you can read somewhere else — not this new one. If you ever forget your password, this is the only way back in without asking an administrator.",
  inviteSubmit: "Set up the account",
  inviteWorking: "Setting up…",
  inviteDoneTitle: "All set",
  inviteGoToSignIn: "Go to sign in",
  inviteFor: (email: string): string => `For ${email}`,
  inviteDoneBody: (email: string): string =>
    `You can sign in as ${email} now, with the password you just chose.`,
  userApps: "Apps",
  userAppsHint:
    "Only the ticked apps appear in this person's navigation, and the server refuses the rest — this does not just hide, it closes. Mail and Home cannot be switched off. Ticking an app does not grant everything inside it: Finance still wants the accountant role, and a Space still wants membership.",
  userAppsSelfHint:
    "This is your own account. An admin is never locked out, so these switches change nothing about what you can open — they are kept in case this account ever stops being an admin.",
  // Shown where a module would have rendered, when it is switched off for
  // this person. Says who can undo it, because the person reading it cannot.
  accessModuleOff: "This app is switched off for your account.",
  accessModuleOffHint: "A workspace administrator can switch it back on.",
  accessBackHome: "Back to Home",
  userAccountantRole: "Accountant",
  userAccountantHint:
    "Reads the books — reports, expense approvals, and closing a period — and can open invoices and deals without changing them. No admin console, and no access to anyone else's mail or files.",
  userAccountantBadge: "Accountant",
  userAliases: "Aliases",
  userAliasesHint: "Extra addresses that deliver to this mailbox.",
  userAliasPlaceholder: "alias@namel3ss.com",
  userAliasAdd: "Add an alias",
  userDelete: "Delete user",
  userDeleteConfirm: (email: string) =>
    `Delete ${email} and all of their mail? This cannot be undone.`,
  userActionError: "That didn't work — please try again.",
  userClose: "Close",
  adminAiProviders: "AI providers",
  adminProviderEnabledFor: (name: string) => `${name} enabled`,
  adminAiIntro:
    "Choose which models power alo — self-hosted, or your own API keys.",
  adminAddProvider: "Add provider",
  adminManage: "Manage",
  adminDefaultBadge: "Default",
  adminMakeDefault: "Make default",
  adminProvidersError: "Couldn't load providers.",
  adminAiSelfHosted: "Self-hosted (recommended)",
  adminAiSelfHostedHint:
    "Runs on your own infrastructure — no data leaves your servers.",
  adminAiOwnKeys: "Your own API keys",
  adminAiOwnKeysHint:
    "Connect an external provider with your key. Requests leave your server to that provider.",
  adminAiFootnote:
    "Self-hosted providers keep all data on your infrastructure. External API keys send requests and content to that provider — choose per your data policy.",
  providerConnected: "Connected",
  providerKeyAdded: "Key added",
  providerReady: "Ready",
  providerNotConfigured: "Not configured",
  kindOllama: "Ollama",
  kindalo: "alo AI",
  kindMistral: "Mistral (EU)",
  kindOpenai: "OpenAI",
  kindAnthropic: "Anthropic",
  kindCustom: "Custom endpoint",
  builtInTag: "Built in",
  ollamaDesc:
    "Local models on your server — Llama 3, Mistral, and more. Fully private.",
  aloDesc:
    "Built-in, EU-hosted model tuned for alo — point it at your alo AI endpoint.",
  mistralDesc:
    "European models, hosted in the EU. Add your Mistral key to enable. Recommended for data sovereignty.",
  openaiDesc: "GPT-4o, GPT-4o mini. Add your OpenAI key to enable.",
  anthropicDesc: "Claude models. Add your Anthropic API key to enable.",
  customDesc:
    "Any OpenAI-compatible API — self-hosted vLLM, Together, Groq, OpenRouter…",
  connectTitle: (name: string) => `Connect ${name}`,
  configureTitle: (name: string) => `Configure ${name}`,
  providerBaseUrl: "API endpoint",
  providerModel: "Model",
  providerModels: "Enabled models",
  providerAddModel: "Add",
  providerModelPlaceholder: "model name",
  providerRemoveModel: (name: string) => `Remove ${name}`,
  providerApiKey: "API key",
  providerShowKey: "Show key",
  providerHideKey: "Hide key",
  providerApiKeyKept: "Saved — leave blank to keep the current key",
  providerApiKeyOptional: "Not needed for a local Ollama",
  providerTest: "Test connection",
  providerTestAgain: "Test again",
  providerTesting: "Testing…",
  providerTestOk: (n: number) =>
    n === 1
      ? "Connection verified — 1 model reachable"
      : `Connection verified — ${n} models reachable`,
  providerTestFail: "Couldn't reach that endpoint.",
  providerCancel: "Cancel",
  providerSave: "Save & enable",
  providerSaveError: "Couldn't save that provider.",
  providerRequired: "An endpoint and a model are required.",
  removeRecipient: (name: string) => `Remove ${name}`,
  recipientCount: (n: number) => (n === 1 ? "1 recipient" : `${n} recipients`),

  // mail — not-yet-built (honest placeholders)
  aiComingSoon: "The AI assistant is coming soon.",
  archiveUnavailable: "There's no archive folder to move this to.",

  // technical authoring (Docs) — equations, code, cross-references
  // alo Docs surface (document chrome)
  docTitle: "Q3 Offer — Proceq",
  docSaved: "Saved to Drive · all changes saved",
  docSaving: "Saving…",
  docViewMode: "Document view",
  docCanvasView: "Canvas",
  docCanvasViewHint: "Flexible canvas view",
  docPageView: "Page",
  docPageViewHint: "Print-style page view",
  docFormattingToolbar: "Document formatting toolbar",
  docMenuFile: "File",
  docMenuEdit: "Edit",
  docMenuInsert: "Insert",
  docMenuFormat: "Format",
  docPrint: "Print",
  docInsertDivider: "Divider",
  docInsertPageBreak: "Page break",
  docZoom: "Document zoom",
  docZoomOut: "Zoom out",
  docZoomIn: "Zoom in",
  docParagraphStyle: "Paragraph style",
  docStyleParagraph: "Paragraph",
  docStyleHeading1: "Heading 1",
  docStyleHeading2: "Heading 2",
  docStyleHeading3: "Heading 3",
  docStyleBulletList: "Bulleted list",
  docStyleNumberedList: "Numbered list",
  docStyleChecklist: "Checklist",
  docTextColor: "Text color",
  docHighlightColor: "Highlight color",
  docHighlightNone: "No highlight",
  docColorDefault: "Default color",
  docColorHex: "Hex",
  docColorOpacity: "Opacity",
  docColorEyedropper: "Pick a color from the screen",
  docBrandColors: "Brand colors",
  docSaveBrandColor: "Save current brand color",
  docRemoveBrandColor: "Remove brand color",
  docColorRed: "Red",
  docColorOrange: "Orange",
  docColorYellow: "Yellow",
  docColorGreen: "Green",
  docColorBlue: "Blue",
  docColorPurple: "Purple",
  docIndent: "Increase indent",
  docOutdent: "Decrease indent",
  docWords: "words",
  docCharacters: "characters",
  docInsertLink: "Insert link",
  docLinkPrompt: "Enter the web address for the selected text",
  docInsertImage: "Insert image",
  docFindReplace: "Find and replace",
  docFind: "Find",
  docReplaceWith: "Replace with",
  docFindNext: "Find next",
  docReplaceAll: "Replace all",
  docPageSetup: "Page setup",
  docPageSize: "Page size",
  docPageLetter: "Letter",
  docPageOrientation: "Orientation",
  docPagePortrait: "Portrait",
  docPageLandscape: "Landscape",
  docPageMargins: "Margins",
  docMarginsNormal: "Normal",
  docMarginsNarrow: "Narrow",
  docMarginsWide: "Wide",
  docHeader: "Header",
  docHeaderPlaceholder: "Header text",
  docFooter: "Footer",
  docFooterPlaceholder: "Footer text",
  docPageNumbers: "Show page number",
  docFontFamily: "Font family",
  docFontSize: "Font size",
  docLineSpacing: "Line spacing",
  docAddComment: "Add comment",
  docComment: "Comment",
  docCommentPlaceholder: "Write a comment…",
  docResolveComment: "Resolve comment",
  docReopenComment: "Reopen comment",
  docSavePdf: "Save as PDF",
  docAiPlaceholder: "Tell the AI what to write or change…",
  docAiPropose: "Draft",
  docAiProposalLabel: "Proposed — review before adding",
  docAiInsert: "Insert",
  docAiDiscard: "Discard",
  docAiUnavailable: "AI isn’t available right now.",
  docAskAi: "Ask AI",
  docEquation: "Equation",
  docEquationHint: "Math formula (LaTeX)",
  docBlockGroupAdvanced: "Advanced",
  docShare: "Share",
  docInsert: "Insert",
  insertEquation: "Equation",
  insertCrossRef: "Cross-reference",
  tbNormalText: "Normal text",
  tbEditing: "Editing",

  // the example spec on the page
  specTitle: "Heat Transfer in the Coateq Panel",
  specSubtitle: "Technical specification · Rev. 3",
  specLead1: "The steady-state flux is governed by",
  specLead2: "across the boundary.",
  specMid:
    "where k is the thermal conductivity and r₁, r₂ are the inner and outer radii. Substituting the measured values:",
  specBcHeading: "Boundary conditions",
  specRefLead: "Combining",
  specRefMid: "with the values in",
  specRefTail: "gives the numbers below.",
  tblSymbol: "Symbol",
  tblValue: "Value",

  // equation editor (modal)
  eqTitle: "Equation",
  eqClose: "Close",
  eqInsert: "Insert",
  eqPlaceholder: "e.g.  E = mc^2",
  eqInputLabel: "LaTeX source",
  eqPreview: "Preview",
  eqEmpty: "Start typing LaTeX above.",
  eqError: (message: string) => `Can't render this LaTeX: ${message}`,
  eqNumbered: "Numbered",
  eqEmptyBlock: "Empty equation — click to edit",
  // equation symbol picker
  eqSearchLabel: "Search symbols",
  eqSearchPlaceholder: "Search symbols — e.g. sum, alpha, arrow",
  eqSearchClear: "Clear search",
  eqNoMatches: "No symbols match your search.",
  eqCatStructures: "Structures",
  eqCatStyles: "Fonts & styles",
  eqCatGreek: "Greek",
  eqCatOperators: "Operators",
  eqCatRelations: "Relations",
  eqCatSets: "Sets & logic",
  eqCatArrows: "Arrows",
  eqCatBigops: "Large operators",
  eqCatCalculus: "Calculus",
  eqCatDelimiters: "Delimiters",
  eqCatMisc: "Symbols",

  // compose — insert math/code into an email
  composeInsertEquation: "Insert equation",
  composeInsertCode: "Insert code block",
  strikethrough: "Strikethrough",
  textColor: "Text color",
  highlight: "Highlight",
  bulletList: "Bulleted list",
  numberedList: "Numbered list",
  alignLeft: "Align left",
  alignCenter: "Align center",
  alignRight: "Align right",
  horizontalRule: "Divider",
  insertImage: "Insert image",
  clearFormatting: "Clear formatting",
  textStyle: "Text style",
  styleQuote: "Quote",
  fontFamily: "Font",
  fontSize: "Font size",
  sizeSmall: "Small",
  sizeNormal: "Normal",
  sizeLarge: "Large",
  sizeHuge: "Huge",
  codeInsertTitle: "Insert code block",
  codeInsertHint: "⌘/Ctrl + Enter to insert",
  codePreviewLabel: "Preview — how it looks in the email",
  insertCancel: "Cancel",
  insertConfirm: "Insert",

  // Docs — the module (browser + editor chrome)
  docsTitle: "alo Docs",
  docsNew: "New document",
  docsEmpty: "No documents yet. Create one to start writing.",
  docsDelete: (title: string) => `Delete ${title}`,
  docsAll: "All documents",
  docsUntitled: "Untitled document",
  docsTitleLabel: "Document title",
  docsSaving: "Saving…",
  docsSaved: "Saved",
  docsSaveError: "Couldn't save",

  // Docs — block editor controls
  blockAdd: "Add a block",
  blockMoveUp: "Move block up",
  blockMoveDown: "Move block down",
  blockDelete: "Delete block",
  blockEmptyHint: "Add a heading, text, equation, code, or table to begin.",

  // heading block
  headingH1: "Heading 1",
  headingH2: "Heading 2",
  headingPlaceholder: "Section heading",
  headingLabel: "Heading text",

  // paragraph block
  paraPlaceholder:
    "Write here. Use the toolbar to insert inline math or a cross-reference.",
  paraLabel: "Paragraph text",
  paraInlineMath: "Inline math",
  paraReference: "Reference",
  paraToolbar: "Insert into this paragraph",

  // table block
  tableHeaderCell: "Column heading",
  tableCell: "Cell",
  tableAddRow: "Add row",
  tableAddColumn: "Add column",
  tableRemoveRow: "Remove row",
  tableRemoveColumn: "Remove column",
  tableBlockLabel: "Editable table",

  // code block
  codeSearchLanguage: "Search language…",
  codeNoLanguage: "No matching language",
  codeCopy: "Copy",
  codeCopied: "Copied",
  codeInputLabel: "Code",
  codePlaceholder: "Paste or type your code…",
  codeWrap: "Word wrap",

  // cross-reference chips + picker
  refSection: "Section",
  refEquation: "Eq.",
  refTable: "Table",
  refFigure: "Figure",
  refBroken: "broken reference",
  refInsert: "Insert cross-reference",
  refInsertTitle: "Insert cross-reference",
  refClose: "Close",
  refNoneOfKind: "Nothing of this kind yet.",
  refTabEquations: "Equations",
  refTabSections: "Sections",
  refTabTables: "Tables",
  refTabFigures: "Figures",

  // Drive (ADR 0027) + Spaces (ADR 0026)
  close: "Close",
  driveMyFiles: "My Files",
  driveSpaces: "Spaces",
  driveLocations: "Drive locations",
  driveTrash: "Trash",
  driveNewFolder: "New folder",
  driveNew: "New",
  driveKindDoc: "Document",
  driveKindSheet: "Sheet",
  driveKindWord: "Word document",
  driveKindExcel: "Excel spreadsheet",
  driveKindSlides: "Slides (PowerPoint)",
  driveKindFolder: "Folder",
  driveNameNew: (kind: string): string => `Name the ${kind.toLowerCase()}`,
  driveNewSpace: "New Space",
  driveNewSpacePrompt: "Name the new Space",
  driveUpload: "Upload",
  driveUploading: "Uploading…",
  driveLoadingFile: (name: string) => `Opening ${name}…`,
  driveOpeningEditor: "your file",
  driveFileOpenFailedTitle: "This file did not open",
  driveFileUnavailable:
    "It may have been moved or deleted. Return to your files and choose another item.",
  driveEditorLoadFailed: (reason: string) =>
    `Drive could not open this file. ${reason}`,
  driveBackToFiles: "Back to files",
  driveLoading: "Loading your files…",
  driveRetry: "Try again",
  driveUnknownError: "The server did not provide a reason.",
  driveLoadFailedTitle: "Your files didn’t load",
  driveLoadFailed: (reason: string): string => `Try again. Server: ${reason}`,
  driveActionFailed: (action: string, reason: string): string =>
    `${action} didn’t finish. Try again. Server: ${reason}`,
  driveMovedToTrash: (name: string): string => `${name} moved to Trash.`,
  driveRestoredFromTrash: (name: string): string => `${name} restored.`,
  driveUndo: "Undo",
  driveSelected: (count: number): string =>
    count === 1 ? "1 item selected" : `${count} items selected`,
  driveSelectItem: (name: string): string => `Select ${name}`,
  driveSelectAll: "Select all visible items",
  driveClearSelection: "Clear selection",
  driveSelectionActions: "Selected item actions",
  driveItemsMovedToTrash: (count: number): string =>
    `${count} items moved to Trash.`,
  driveItemsRestored: (count: number): string => `${count} items restored.`,
  drivePurgeManyConfirm: (count: number): string =>
    `Permanently delete ${count} items? This cannot be undone.`,
  driveVersionsLoadFailed: (reason: string): string =>
    `Version history didn’t load. Try again. Server: ${reason}`,
  driveMembersLoadFailed: (reason: string): string =>
    `Members didn’t load. Try again. Server: ${reason}`,
  driveMembers: "Members",
  driveActions: "Actions",
  driveEmpty: "This folder is empty. Upload a file or create a folder.",
  driveEmptyTitle: "Nothing here yet",
  driveEmptyReadOnly: "This space does not contain any files yet.",
  driveEmptyTrashTitle: "Trash is empty",
  driveFolderEmpty: "This folder is empty",
  driveUploadHere: "Upload here",
  driveFolderLoading: (name: string): string => `Loading ${name}…`,
  driveFolderLoadFailed: (reason: string): string =>
    `This folder didn’t load. Server: ${reason}`,
  driveSpacesLoadFailed: (reason: string): string =>
    `Your Spaces didn’t load. Try again. Server: ${reason}`,
  driveSort: "Sort",
  driveSortNameAsc: "Name (A–Z)",
  driveSortNameDesc: "Name (Z–A)",
  driveSortNewest: "Newest first",
  driveSortOldest: "Oldest first",
  driveSortLargest: "Largest first",
  driveSortSmallest: "Smallest first",
  driveView: "View",
  driveViewExtraLarge: "Extra large icons",
  driveViewLarge: "Large icons",
  driveViewMedium: "Medium icons",
  driveViewSmall: "Small icons",
  driveViewList: "List",
  driveViewDetails: "Details",
  driveViewTiles: "Tiles",
  driveViewContent: "Content",
  driveViewNavigationPane: "Navigation pane",
  driveViewCompact: "Compact view",
  driveViewExtensions: "File name extensions",
  driveEmptyTrash: "Trash is empty.",
  driveColName: "Name",
  driveColSize: "Size",
  driveColModified: "Modified",
  driveDetailsTitle: "Details",
  driveDetailsShow: (name: string): string => `Details for ${name}`,
  driveOpen: "Open",
  driveDownload: "Download",
  driveRename: "Rename",
  driveMove: "Move",
  driveCopy: "Make a copy",
  driveVersionHistory: "Version history",
  driveTrashAction: "Move to Trash",
  driveRestore: "Restore",
  driveDeleteForever: "Delete forever",
  driveNewFolderPrompt: "Name the new folder",
  driveRenamePrompt: "New name",
  driveTrashConfirm: (name: string) => `Move “${name}” to Trash?`,
  drivePurgeConfirm: (name: string) =>
    `Permanently delete “${name}”? This cannot be undone.`,
  driveMoveTo: "Move to…",
  driveCopyTo: "Copy to…",
  driveDestHint: "The item takes on the access of wherever you put it.",
  driveNoVersions: "No previous versions.",
  driveCurrent: "Current",
  driveMembersOf: (name: string) => `Members of ${name}`,
  // The return is annotated rather than inferred. Without it TypeScript reads
  // the type as the literal union `"Manager" | "Editor" | "Viewer"`, and since
  // the catalog's shape comes from this file, no translation of the three words
  // can satisfy it — the English is the type. A label is a string.
  driveRole: (role: string): string =>
    role === "manager" ? "Manager" : role === "editor" ? "Editor" : "Viewer",
  driveAddMemberPlaceholder: "Add someone by email",
  // The name of the two add-a-member controls, for a screen reader. A
  // placeholder disappears the moment you type into it, and a select with no
  // label is announced as whichever role it currently shows.
  driveAddMemberLabel: "Email address",
  driveMemberRoleLabel: "Role",
  driveAdd: "Add",
  driveRemoveMember: "Remove",
  /** Names the person a Remove button takes out — a column of buttons all
   *  called "Remove" tells a screen-reader user nothing. */
  driveRemoveMemberFor: (who: string): string => `Remove ${who}`,
  driveRemoveMemberConfirm: (who: string) => `Remove ${who} from this Space?`,
  driveMemberError: "Couldn’t add that person — check the email and your role.",
  driveNewDoc: "New doc",
  driveCreateDocument: "New document",
  driveCreateMore: "More creation options",
  driveNewDocPrompt: "Name the new doc",
  driveNewSheetPrompt: "Name the new sheet",
  driveImporting: (name: string): string => `Importing ${name}…`,
  driveImportNote:
    "We’re opening this as an alo Sheet. Some formatting may differ — your original file stays in Drive, unchanged.",
  driveImportFailed: (name: string): string =>
    `Couldn’t import ${name}. You can still download the original.`,
  sheetDownloadXlsx: "Download as Excel (.xlsx)",
  sheetDownloadXlsxShort: "Excel",
  sheetName: "Sheet name",
  sheetLoading: "Loading your sheet…",
  sheetLoadFailedTitle: "This sheet didn’t load",
  docLoading: "Loading your document…",
  docLoadFailedTitle: "This document didn’t load",
  docSaveFailed: (reason: string): string =>
    `Your latest changes are not saved yet. Choose Retry to save them. Server: ${reason}`,
  sheetSaveFailed: (reason: string): string =>
    `Your latest changes are not saved yet. We’ll keep trying. Server: ${reason}`,
  sheetSaved: "Saved",
  sheetExport: "Export",
  sheetMore: "More actions",
  sheetRibbon: "Formatting",
  sheetTabHome: "Home",
  sheetTabOthers: "Others",
  sheetTabInsert: "Insert",
  sheetTabDraw: "Draw",
  sheetTabLayout: "Page Layout",
  sheetTabFormulas: "Formulas",
  sheetTabData: "Data",
  sheetTabReview: "Review",
  sheetTabView: "View",
  sheetTabSoon: (name: string): string => `${name} tools are coming soon.`,
  sheetGroupCellSize: "Cell Size",
  sheetRowHeight: "Row height",
  sheetColumnWidth: "Column width",
  sheetAutoFitRow: "Auto-fit row",
  sheetAutoFitColumn: "Auto-fit column",
  sheetGroupVisibility: "Visibility",
  sheetHideRow: "Hide selected row",
  sheetShowRows: "Show all rows",
  sheetHideColumn: "Hide selected column",
  sheetShowColumns: "Show all columns",
  sheetGroupSheetOptions: "Sheet Options",
  sheetToggleGridlines: "Gridlines",
  sheetGridlineColor: "Gridline colour",
  sheetGroupDirection: "Direction",
  sheetLeftToRight: "Left to right",
  sheetRightToLeft: "Right to left",
  sheetUndo: "Undo",
  sheetRedo: "Redo",
  sheetGroupHistory: "Undo",
  sheetGroupFont: "Font",
  sheetGroupBorders: "Borders",
  sheetGroupRotation: "Rotation",
  sheetGroupAlignment: "Alignment",
  sheetGroupWrap: "Wrapping",
  sheetGroupMerge: "Merge",
  sheetWrapOverflow: "Overflow",
  sheetWrapText: "Wrap",
  sheetWrapClip: "Clip",
  sheetMergeAll: "Merge all",
  sheetMergeAcross: "Merge across",
  sheetMergeVertically: "Merge vertically",
  sheetUnmerge: "Unmerge",
  sheetGroupNumber: "Number",
  sheetFontFamily: "Font",
  sheetFontSize: "Font size",
  sheetBold: "Bold",
  sheetItalic: "Italic",
  sheetUnderline: "Underline",
  sheetStrike: "Strikethrough",
  sheetAlignLeft: "Align left",
  sheetAlignCenter: "Align center",
  sheetAlignRight: "Align right",
  sheetMerge: "Merge cells",
  sheetNumberFormat: "Number format",
  sheetCellStyles: "Cell styles",
  sheetMoreStyles: "More cell styles",
  sheetStyleDefault: "Default",
  sheetStyleHeading1: "Heading 1",
  sheetStyleHeading2: "Heading 2",
  sheetStyleHeading3: "Heading 3",
  sheetStyleHeading4: "Heading 4",
  sheetStyleTitle: "Title",
  sheetStyleSubtitle: "Subtitle",
  sheetFormatGeneral: "General",
  sheetFormatNumber: "Number",
  sheetFormatCurrency: "Currency",
  sheetFormatPercentage: "Percentage",
  sheetFormatDate: "Date",
  sheetFormatText: "Text",
  sheetFormatPreviewGeneral: "1234.56",
  sheetFormatPreviewNumber: "1,234.56",
  sheetFormatPreviewCurrency: "€ 1,234.56",
  sheetFormatPreviewPercentage: "12.34%",
  sheetFormatPreviewDate: "2026-08-06",
  sheetFormatPreviewText: "Text",
  sheetFontGrow: "Increase font size",
  sheetFontShrink: "Decrease font size",
  sheetFontColor: "Text colour",
  sheetFillColor: "Fill colour",
  sheetAlignTop: "Align top",
  sheetAlignMiddle: "Align middle",
  sheetAlignBottom: "Align bottom",
  sheetWrap: "Wrap text",
  sheetGroupCells: "Cells",
  sheetInsert: "Insert",
  sheetDelete: "Delete",
  sheetFormat: "Format",
  sheetMoreCellOptions: "More cell options",
  sheetSortFilter: "Sort & Filter",
  sheetGroupClear: "Clear",
  sheetGroupRows: "Rows",
  sheetGroupColumns: "Columns",
  sheetGroupView: "Window",
  sheetInsertRowAbove: "Insert row above",
  sheetInsertRowBelow: "Insert row below",
  sheetInsertColLeft: "Insert column left",
  sheetInsertColRight: "Insert column right",
  sheetDeleteRow: "Delete row",
  sheetDeleteColumn: "Delete column",
  sheetClearContents: "Clear contents",
  sheetClearFormats: "Clear formatting",
  sheetFreeze: "Freeze panes",
  sheetUnfreeze: "Unfreeze",
  sheetGroupClipboard: "Clipboard",
  sheetGroupStyles: "Styles",
  sheetGroupEditing: "Editing",
  sheetGroupSortFilter: "Sort & Filter",
  sheetGroupDataTools: "Data tools",
  sheetGroupCharts: "Charts",
  sheetChartBar: "Bar chart",
  sheetChartLine: "Line chart",
  sheetChartPie: "Pie chart",
  sheetCharts: "Charts in this sheet",
  sheetChartRemove: "Remove chart",
  sheetChartSelectionHint:
    "Select a header row, a category column, and at least one numeric series.",
  sheetChartExcelLimit:
    "Charts stay live in alo Sheet. Excel export currently includes the cells, but not these charts.",
  sheetChartSeries: (number: number) => `Series ${number}`,
  chartTabMissing: "The sheet tab used by this chart no longer exists.",
  chartRangesRagged: "The chart ranges no longer have matching lengths.",
  chartTooLarge: "This chart selection is too large to draw safely.",
  sheetGroupProtection: "Protection",
  sheetGroupFreeze: "Freeze panes",
  sheetGroupZoom: "Zoom",
  sheetGroupInsertObjects: "Objects",
  sheetGroupDrawing: "Drawing",
  sheetGroupNotes: "Notes",
  sheetGroupComments: "Comments",
  sheetGroupFunctionLibrary: "Function library",
  sheetGroupMoreFunctions: "More functions",
  sheetAutoSum: "AutoSum",
  sheetAverage: "Average",
  sheetCount: "Count",
  sheetMinimum: "Minimum",
  sheetMaximum: "Maximum",
  sheetMoreFunctions: "Browse functions",
  sheetGroupFunctionCategories: "Function categories",
  sheetFormulaFinancial: "Financial",
  sheetFormulaDateTime: "Date & Time",
  sheetFormulaMathTrig: "Math & Trig",
  sheetFormulaStatistical: "Statistical",
  sheetFormulaLookup: "Lookup & Reference",
  sheetFormulaDatabase: "Database",
  sheetFormulaText: "Text",
  sheetFormulaLogical: "Logical",
  sheetFormulaInformation: "Information",
  sheetFormulaEngineering: "Engineering",
  sheetFormulaCube: "Cube",
  sheetFormulaCompatibility: "Compatibility",
  sheetFormulaWeb: "Web",
  sheetFormulaArray: "Array",
  sheetDataValidation: "Data validation",
  sheetConditionalFormatting: "Conditional formatting",
  sheetTextToColumns: "Text to columns",
  sheetNamedRanges: "Named ranges",
  sheetProtectRange: "Protect range",
  sheetUnprotectRange: "Unprotect range",
  sheetProtectSheet: "Protect sheet",
  sheetUnprotectSheet: "Unprotect sheet",
  sheetProtectedRangeName: "Protected range",
  sheetProtectedSheetName: "Protected sheet",
  sheetFreezeTopRow: "Freeze top row",
  sheetFreezeFirstColumn: "Freeze first column",
  sheetZoomOut: "Zoom out",
  sheetZoomReset: "100%",
  sheetZoomIn: "Zoom in",
  sheetInsertTable: "Table",
  sheetInsertLink: "Link",
  sheetInsertImage: "Image",
  sheetDrawingPanel: "Images and drawing",
  sheetNote: "Add or edit note",
  sheetAddComment: "New comment",
  sheetCommentsPanel: "Comments panel",
  sheetPaste: "Paste",
  sheetCut: "Cut",
  sheetCopy: "Copy",
  sheetPercent: "Percent",
  sheetCurrency: "Currency",
  sheetComma: "Thousands separator",
  sheetSortAsc: "Sort A → Z",
  sheetSortDesc: "Sort Z → A",
  sheetFilter: "Toggle filter",
  sheetFindReplace: "Find & Replace",
  sheetBorders: "Borders",
  sheetBordersAll: "All borders",
  sheetBordersOuter: "Outer border",
  sheetBordersInside: "Inside borders",
  sheetBordersTop: "Top border",
  sheetBordersBottom: "Bottom border",
  sheetBordersLeft: "Left border",
  sheetBordersRight: "Right border",
  sheetBordersHorizontal: "Horizontal borders",
  sheetBordersVertical: "Vertical borders",
  sheetBordersNone: "No border",
  sheetBordersAdvanced: "Diagonal borders",
  sheetBordersDiagonalDown: "Diagonal down border",
  sheetBordersDiagonalUp: "Diagonal up border",
  sheetBordersDiagonalDownCenter: "Diagonal down with centre lines",
  sheetBordersDiagonalDownBoth: "Diagonal down with both centre lines",
  sheetBordersDiagonalUpCenter: "Diagonal up with centre lines",
  sheetRotation: "Rotation",
  sheetRotationNone: "No rotation",
  sheetRotation45: "Rotate 45° clockwise",
  sheetRotationMinus45: "Rotate 45° counter-clockwise",
  sheetRotation90: "Rotate 90° clockwise",
  sheetRotationMinus90: "Rotate 90° counter-clockwise",
  sheetRotationVertical: "Vertical text",
  officeUnavailable:
    "This document couldn’t be opened for editing. Try again, or download it.",
  officeLoading: "Opening the Office editor…",
  officeDiscoveryMissing:
    "The Office editor did not publish an editor address.",
  officeLoadFailed: (reason: string): string => `Try again. Server: ${reason}`,
  moduleSearch: "Search",
  searchPlaceholder: "Search files, tasks and email…",
  searchHint: "Search files and tasks by name, and email by content.",
  searchNoResults: "Nothing found.",
  aiAskAbout: (q: string): string => `Ask AI: “${q}”`,
  aiSources: "Sources",
  aiUnconfigured:
    "AI isn’t set up yet — an admin can add a model. Here’s what matched:",
  aiUnreachable: "The AI couldn’t be reached. Here’s what matched:",
  // alo Chat (ADR 0038).
  chatNewChannel: "New channel",
  chatNewChannelPrompt: "Give it a short, obvious name — people join by it.",
  chatNewChannelPlaceholder: "e.g. product-launch",
  chatCreate: "Create",
  chatDirectMessage: "Direct message",
  chatLoading: "Loading…",
  chatSend: "Send",
  chatEdited: "edited",
  chatWithdrawn: "This message was withdrawn.",
  chatMessageSent: "Sent",
  chatMessageReadBy: (count: number) => `Read by ${count}`,
  chatNoMessagesYet: "No messages yet — say the first thing.",
  chatArchived: "Archived",
  chatReplyInThread: "Reply here",
  chatReplyHere: "Reply here",
  chatReplyPrivately: "Reply privately",
  chatReplyingHere: "Replying here",
  chatReplyingPrivately: (who: string): string =>
    `Replying privately to ${who}`,
  chatCancelReply: "Cancel reply",
  chatAddReaction: "Add a reaction",
  chatAgentTag: "agent",
  chatOlder: "Show earlier messages",
  chatBrowse: "Browse channels",
  chatNewDm: "New conversation",
  chatFindPerson: "Find a colleague",
  chatFindPersonHint: "Type at least two letters of their address.",
  chatNobodyFound: "Nobody here matches that.",
  chatPeopleFailed: "That search couldn’t be run.",
  chatDmFailed: "That conversation couldn’t be started.",
  chatJoin: "Join",
  chatJoined: "Open",
  chatNothingToJoin: "No public channels in this workspace yet.",
  chatBrowseFailed: "Those channels couldn’t be listed.",
  chatJoinFailed: "That channel couldn’t be joined.",
  chatEditAction: "Edit",
  chatWithdrawAction: "Withdraw",
  chatEditLabel: "Edit this message",
  chatEditSave: "Save",
  chatEditCancel: "Cancel",
  chatEditFailed: "That edit couldn’t be saved.",
  chatWithdrawFailed: "That message couldn’t be withdrawn.",
  chatWhoIsHere: "Who's here",
  chatMembersAndAgents: "Members & agents",
  chatThinking: (handle: string): string => `@${handle} is thinking`,
  chatStop: "Stop",
  chatBold: "Bold",
  chatItalic: "Italic",
  chatInlineCode: "Code",
  chatCodeBlock: "Code block",
  chatCodeBlockHint: "Insert a formatted block for code or commands.",
  chatFormulaHint: "Insert a mathematical formula.",
  chatFormatting: "Text formatting",
  chatFormula: "Formula",
  chatBulletList: "Bulleted list",
  chatQuoteAction: "Quote",
  chatFormatHint: "text",
  meetTitle: "Meeting",
  meetEyebrow: "Your meeting space",
  meetSubtitle: "Start a call or step into one that is already happening.",
  meetHeroTitle: "Be together in one click",
  meetHeroText:
    "Microphone on, camera your choice. Check both before anyone sees or hears you.",
  meetHappeningNow: "Happening now",
  meetHappeningHint: "Meetings you can join without asking for a link.",
  meetLiveCount: (count: number) =>
    count === 1 ? "1 meeting" : `${count} meetings`,
  meetReady: "Ready",
  meetStartedAt: (time: string) => `Started at ${time}`,
  meetStartNow: "Start a meeting",
  meetStarting: "Starting…",
  meetStartFailed:
    "The meeting couldn’t be started. Check your connection and try again.",
  meetLoading: "Loading meetings",
  meetLoadFailed: "Meetings couldn’t be loaded",
  meetLoadFailedHint:
    "Check your connection, then try again. Starting a new meeting is still available.",
  meetRetry: "Try again",
  meetBack: "Back to Meet",
  meetInstantTitle: "Instant meeting",
  meetNothingLive: "No meetings are running",
  meetWhereFrom:
    "Meetings usually start where the people are — in a conversation, or on a calendar invitation. Anything running that you can join appears here.",
  meetUntitled: "Instant meeting",
  meetNotStarted: "Not started yet",
  meetAddToEvent: "Add a meeting",
  meetStart: "Start a meeting",
  meetStartedHere: "started a meeting in this conversation",
  chatMeetingPreview: "Started a meeting",
  meetJoin: "Join the meeting",
  meetLive: "Meeting in progress",
  meetJoinNow: "Join now",
  meetReadyGreeting: (name: string) => (name ? `Hi ${name}` : "Hi there"),
  meetReadyTitle: "You’re all set to join",
  meetReadyBody: "Check your camera and microphone before you join.",
  meetReadySafetyTitle: "Your meeting is safe",
  meetReadySafetyBody:
    "Only invited people and participants admitted by the host can join.",
  meetSettingsAfterJoin: "You can still change your settings after joining.",
  meetGoodConnection: "Good connection",
  meetConnectingStatus: "Connecting",
  meetEnterFullscreen: "Enter full screen",
  meetExitFullscreen: "Exit full screen",
  meetMicrophone: "Microphone",
  meetCamera: "Camera",
  meetJoining: "Joining…",
  meetLeave: "Leave",
  meetRecord: "Record",
  meetRecording: "Recording",
  meetStartRecording: "Start recording",
  meetStopRecording: "Stop recording",
  meetIConsent: "I consent",
  meetRecordingConsentTitle: "Recording needs everyone's consent",
  meetRecordingConsentBody:
    "The host can start after everyone currently in the room agrees.",
  meetRecordingConsentGiven: "Consent given",
  meetConsentCount: (count: number) => `${count} consented`,
  meetRecordingFailed: "The recording action could not be completed.",
  meetGenerateMinutes: "Create minutes",
  meetMinutesTitle: "Meeting minutes",
  meetMinutesActions: "Action items",
  meetMinutesNoActions: "No action items were identified.",
  meetMinutesFailed: "Minutes need a transcript and a configured AI provider.",
  meetPresentingTitle: "You’re presenting",
  meetPresentingBody:
    "Everyone else sees your shared screen. You see this calm reminder instead of a hall of mirrors.",
  meetClose: "Close",
  meetJoinFailed: "That meeting couldn’t be joined.",
  meetJoinProblemTitle: "We couldn’t connect you",
  meetUnavailableTitle: "Meet needs one last connection",
  meetRaiseHand: "Raise hand",
  meetLowerHand: "Lower hand",
  meetReact: "Send a reaction",
  meetInvite: "Invite",
  meetInviteTitle: "Join my alo meeting",
  meetInviteText: "Use this alo link to join the meeting.",
  meetChatEmptyTitle: "The room is listening",
  meetChatEmptyBody:
    "Share a thought, a link, or the detail everyone will want after the call.",
  meetChat: "Chat",
  meetCaptions: "Live captions",
  meetCaptionLanguage: "Caption language",
  meetCaptionOriginal: "Original",
  meetToolLoading: "Loading meeting tools…",
  meetAgenda: "Agenda",
  meetAgendaHint: "Keep the room aligned on what comes next.",
  meetAgendaPlaceholder: "Add an agenda item",
  meetPolls: "Polls",
  meetPollsHint: "Ask the room and see the answer together.",
  meetPollQuestion: "Question",
  meetPollOptionOne: "First option",
  meetPollOptionTwo: "Second option",
  meetCreatePoll: "Create poll",
  meetNotes: "Notes",
  meetNotesHint: "Shared notes that stay with this meeting.",
  meetNotesPlaceholder: "Capture decisions, context, and follow-up…",
  meetFiles: "Files",
  meetFilesHint: "Images and PDFs shared in this call.",
  meetNoFiles: "No files have been shared yet.",
  meetToolsFailed: "Meeting tools changed elsewhere. Reload and try again.",
  deleteLabel: "Delete",
  add: "Add",
  save: "Save",
  meetCaptionsWaiting:
    "Captions will appear when the transcription service hears speech.",
  meetChatTitle: "In-call chat",
  meetChatMessages: "Messages",
  meetChatPeople: (count: number) => `People (${count})`,
  meetChatPlaceholder: "Send a message",
  meetMessageSendFailed:
    "That message was not saved. Check your connection and try again.",
  meetEveryone: "Everyone",
  meetSendTo: "To:",
  meetChooseRecipient: "Send message to",
  meetEveryoneHint: "Visible to everyone in the meeting",
  meetPrivateHint: "Only this person will receive it",
  meetPrivate: "Private",
  meetReplyPrivately: "Reply privately",
  meetMessagePrivately: "Message privately",
  meetAttachFile: "Add an image or PDF",
  meetAddEmoji: "Add emoji",
  meetSettings: "Meeting settings",
  meetDeviceSettings: "Camera and audio",
  meetDeviceSettingsHint:
    "Changes take effect immediately and stay on this device.",
  meetBackgroundEffects: "Background effects",
  meetBackgroundEffectsHint:
    "Keep the focus on you. The effect is applied to the video everyone receives.",
  meetBackgroundNone: "None",
  meetBackgroundBlur: "Blur",
  meetBackgroundUnsupported:
    "Background blur is not supported by this browser or camera.",
  meetReconnecting: "Reconnecting your call",
  meetReconnectingHint:
    "Stay here — audio and video will resume automatically.",
  meetConnectionLost: "The call lost its connection. Try joining again.",
  meetPictureInPicture: "Picture in picture",
  meetSpeaker: "Speaker",
  meetDone: "Done",
  meetYou: "You",
  meetParticipant: "Participant",
  meetHost: "Host",
  meetSpeaking: "Speaking",
  meetMuted: "Muted",
  meetMuteParticipant: "Mute participant",
  meetRemoveParticipant: "Remove participant",
  meetRemoveParticipantConfirm: (name: string) =>
    `Remove ${name} from this meeting?`,
  meetModerationFailed:
    "That participant action could not be completed. Try again.",
  meetQuickReplyOne: "👍 Sounds good",
  meetQuickReplyTwo: "Let's go!",
  meetQuickReplyThree: "Start now",
  meetJoinPlaceholder: "Enter a meeting code or alo link",
  meetJoinShort: "Join",
  meetNew: "New meeting",
  meetYourSpaceLead: "Your",
  meetYourSpaceAccent: "meeting space",
  meetHeroNewTitle: "Meet together in one click",
  meetHeroNewText:
    "High-quality calls with screen sharing, chat, reactions, and a device check before anyone sees or hears you.",
  meetSchedule: "Schedule",
  meetJoinInputInvalid: "Enter a valid alo meeting link or meeting code.",
  meetUpcoming: "Upcoming meetings",
  meetUpcomingHint: "What your calendar says is next.",
  meetRecent: "Recent meetings",
  meetRecentHint: "Calls you could attend, kept as workspace history.",
  meetEndedAt: (time: string) => `Ended ${time}`,
  meetDuration: (minutes: number) => `${minutes} min`,
  meetCalendarUntitled: "Untitled calendar event",
  meetSafetyTitle: "Entry stays under your control",
  meetSafetyBody:
    "The workspace checks access before it issues a media token. A meeting code never bypasses authorization.",
  meetTodaySchedule: "Today’s schedule",
  meetOpenAgenda: "Open Agenda",
  meetNoEventsToday: "Nothing else is scheduled for today.",
  meetViewAgenda: "View full Agenda",
  meetQuickActions: "Quick actions",
  meetLinkCopied: "Link copied",
  meetSomeone: "Someone",
  meetHandsRaised: (names: string) => `Hand raised: ${names}`,
  meetNoEngine:
    "Meetings aren’t switched on for this workspace yet. The meeting is recorded and everyone invited can see it — there is just nowhere to hold it until an administrator configures the meeting server.",
  chatBackToList: "Back to conversations",
  chatJumpTo: "Jump to a conversation",
  chatNoRoom: "No conversation matches that.",
  chatDropFiles: "Drop to share from your computer",
  chatNewMessages: "New messages",
  chatToday: "Today",
  chatYesterday: "Yesterday",
  chatBeginning: (name: string): string => `This is the beginning of ${name}`,
  chatBeginningDm: "This is the beginning of your conversation",
  chatSectionChannels: "Channels",
  chatFilterAll: "All",
  chatFilterUnread: "Unread",
  chatFilterThreads: "Threads",
  chatFilterMentions: "Mentions",
  chatCompose: "Compose",
  chatSectionDirect: "Direct messages",
  chatSectionArchived: "Archived",
  chatChannelActions: (name: string): string => `Actions for ${name}`,
  chatRename: "Rename channel",
  chatRenamePrompt: "Everyone in the channel sees the new name.",
  chatRenameSave: "Rename",
  chatRenameFailed: "That channel couldn’t be renamed.",
  chatAddDescription: "Add description",
  chatEditDescription: "Edit description",
  chatDescriptionPrompt: "Help people understand what this channel is for.",
  chatDescriptionSave: "Save description",
  chatDescriptionFailed: "That channel description couldn’t be saved.",
  chatArchiveAction: "Archive channel",
  chatArchiveTitle: (name: string): string => `Archive ${name}?`,
  chatArchiveWarning:
    "Nothing is deleted. The history stays readable, but no one can post here again.",
  chatArchiveConfirm: "Archive",
  chatArchiveFailed: "That channel couldn’t be archived.",
  chatClose: "Close",
  chatOwner: "owner",
  chatAgentsHere: "Agents in this conversation",
  chatAgentNothingYet: "Hasn’t been asked anything yet",
  chatAgentRecord: (answers: number, actions: number): string => {
    const said = answers === 1 ? "1 answer" : `${answers} answers`;
    if (actions === 0) return said;
    return `${said} · ${actions === 1 ? "1 action" : `${actions} actions`} approved`;
  },
  chatAgentsAvailable: "Available to add",
  chatNoAgentsHere: "No agents here yet. Add one and mention it by name.",
  chatPeopleHere: "People",
  chatAgentAdd: (handle: string): string =>
    `Add @${handle} to this conversation`,
  chatAgentRemove: (handle: string): string => `Remove @${handle}`,
  chatAgentAddFailed: "That agent couldn’t be added.",
  chatAgentRemoveFailed: "That agent couldn’t be removed.",
  agentMemoryTitle: (handle: string): string => `What @${handle} remembers`,
  agentMemoryShared:
    "Learned in this conversation. Everyone here can read this list.",
  agentMemoryAboutYou:
    "What it remembers about you from this one-to-one. Only you can see this list.",
  agentMemoryEmpty:
    "Nothing remembered yet. Facts it learns here — and anything you ask it to remember — appear in this list.",
  agentMemoryExplicit: "Told directly",
  agentMemoryFromTurn: "Learned from an answer",
  agentMemoryForget: "Forget",
  agentMemoryForgetFact: (fact: string): string => `Forget “${fact}”`,
  agentMemoryLoadFailed: "What it remembers couldn’t be loaded.",
  agentMemoryForgetFailed: "That couldn’t be forgotten.",
  agentInstructionsTitle: "Standing instructions",
  agentInstructionsIntro:
    "Asked once, in advance. Each one runs as the person who asked it, and everyone here can read this list.",
  agentInstructionsEmpty:
    "Nothing standing yet. Pick an agent, say what it should do, choose how often — it runs your words on that schedule and posts here.",
  agentInstructionHourly: "Runs every hour",
  agentInstructionDaily: "Runs every day",
  agentInstructionWeekly: "Runs every week",
  agentInstructionEveryHours: (hours: number): string =>
    `Runs every ${hours} hours`,
  agentInstructionEveryMinutes: (minutes: number): string =>
    `Runs every ${minutes} minutes`,
  agentInstructionOnEvent: (verb: string): string =>
    `Runs after every “${verb}”`,
  agentInstructionNextRun: (at: string): string => `Next run ${at}`,
  agentInstructionAskedBy: (who: string): string => `Asked by ${who}`,
  agentInstructionPaused: "Paused — the person who asked left the room.",
  agentInstructionCancel: "Cancel",
  agentInstructionCancelThis: (text: string): string => `Cancel “${text}”`,
  agentInstructionAgentLabel: "Agent",
  agentInstructionTextLabel: "What should it do?",
  agentInstructionTextPlaceholder: "e.g. list the invoices that fell overdue",
  agentInstructionScheduleLabel: "How often",
  agentInstructionOptionHourly: "Every hour",
  agentInstructionOption4Hours: "Every 4 hours",
  agentInstructionOptionDaily: "Every day",
  agentInstructionOptionWeekly: "Every week",
  agentInstructionAdd: "Add instruction",
  agentInstructionsLoadFailed: "The standing instructions couldn’t be loaded.",
  agentInstructionCreateFailed: "That instruction couldn’t be added.",
  agentInstructionCancelFailed: "That couldn’t be cancelled.",
  recordAgentTitle: "This record’s agent",
  recordAgentOriginNone: "This record doesn’t say where it came from.",
  recordAgentOriginPerson: (who: string): string => `Created by ${who}.`,
  recordAgentOriginThread: (room: string): string =>
    `Captured from the “${room}” conversation.`,
  recordAgentOriginThreadUnnamed: "Captured from a conversation.",
  recordAgentOriginEmail: "Raised from an email.",
  recordAgentOriginEvent: "From a calendar event.",
  recordAgentOriginQuote: (quote: string): string =>
    `Raised from quote ${quote}.`,
  recordAgentOriginFrom: (source: string): string => `From ${source}.`,
  recordAgentOpenSource: "Open it",
  recordAgentCanDo: (handle: string): string => `What @${handle} can do here`,
  recordAgentAskPlaceholder: (handle: string): string =>
    `Ask @${handle} about this…`,
  recordAgentAsk: "Ask",
  recordAgentAsking: (handle: string): string => `Asking @${handle}…`,
  recordAgentNoAnswerYet:
    "No answer yet — it will land in the conversation.",
  recordAgentOpenConversation: "Open the conversation",
  recordAgentAskFailed: "That couldn’t be asked.",
  recordAgentVerbFailed: "That couldn’t be started.",
  recordAgentAskAbout: (record: string, question: string): string =>
    `About “${record}”: ${question}`,
  recordAgentVerbChaseTask: "Chase it",
  recordAgentVerbSetTaskPriority: "Set its priority",
  recordAgentVerbCompleteTask: "Mark it done",
  recordAgentVerbReassignTask: "Hand it over",
  recordAgentDraftChaseTask: (task: string): string => `Chase “${task}”.`,
  recordAgentDraftSetTaskPriority: (task: string): string =>
    `Set the priority of “${task}” to `,
  recordAgentDraftCompleteTask: (task: string): string =>
    `Mark “${task}” as done.`,
  recordAgentDraftReassignTask: (task: string): string =>
    `Reassign “${task}” to `,
  recordAgentVerbMoveDealStage: "Move its stage",
  recordAgentVerbDraftFollowup: "Draft a follow-up",
  recordAgentDraftMoveDealStage: (deal: string): string =>
    `Move “${deal}” to the stage `,
  recordAgentDraftDraftFollowup: (deal: string): string =>
    `Draft a follow-up for “${deal}”.`,
  recordAgentVerbApproveExpense: "Approve it",
  recordAgentVerbSuggestCategories: "Suggest categories",
  recordAgentDraftApproveExpense: (merchant: string): string =>
    `Approve the expense “${merchant}”.`,
  recordAgentDraftSuggestCategories:
    "Go through my expense claims without a category and suggest one for each.",
  recordAgentVerbProjectStatus: "Sum up its status",
  recordAgentVerbLogTime: "Log time on it",
  recordAgentVerbDraftTimesheet: "Draft it from my calendar",
  recordAgentDraftProjectStatus: (project: string): string =>
    `Sum up the status of “${project}”.`,
  recordAgentDraftLogTime: (project: string): string =>
    `Log time on “${project}”: `,
  recordAgentDraftDraftTimesheet: (week: string): string =>
    `Draft my timesheet for ${week} from my calendar.`,
  recordAgentVerbReceiveDelivery: "Receive its delivery",
  recordAgentDraftReceiveDelivery: (order: string): string =>
    `Receive the delivery for “${order}”.`,
  recordAgentVerbApproveLeave: "Approve it",
  recordAgentVerbDraftLetter: "Draft a letter",
  recordAgentDraftApproveLeave: (person: string): string =>
    `Approve the leave request from “${person}”.`,
  recordAgentDraftDraftLetter: (person: string): string =>
    `Draft a letter for “${person}” from a template.`,
  recordAgentOriginImport: (format: string): string =>
    `Imported from a ${format} file.`,
  recordAgentPanelToggle: "Its agent",
  recordAgentFocusRecord: (record: string): string =>
    `The agent for “${record}”`,
  recordAgentVerbRenameFile: "Rename it",
  recordAgentDraftRenameFile: (file: string): string =>
    `Rename “${file}” to `,
  recordAgentVerbMoveFile: "Move it",
  recordAgentDraftMoveFile: (file: string): string =>
    `Move “${file}” into the folder `,
  recordAgentVerbListFolder: "List what’s in it",
  recordAgentDraftListFolder: (folder: string): string =>
    `What’s in the “${folder}” folder?`,
  recordAgentVerbDraftSection: "Draft a section",
  recordAgentDraftDraftSection: (document: string): string =>
    `Draft a section for “${document}” about `,
  recordAgentVerbRewriteDoc: "Rewrite a passage",
  recordAgentDraftRewriteDoc: (document: string): string =>
    `In “${document}”, rewrite the passage about `,
  recordAgentVerbWriteFormula: "Write a formula",
  recordAgentDraftWriteFormula: (sheet: string): string =>
    `In “${sheet}”, write a formula that `,
  recordAgentVerbTidyColumn: "Tidy a column",
  recordAgentDraftTidyColumn: (sheet: string): string =>
    `In “${sheet}”, tidy column `,
  recordAgentVerbMeetingPrep: "Prepare for it",
  recordAgentDraftMeetingPrep: (meeting: string): string =>
    `What do I need for “${meeting}”?`,
  recordAgentVerbRescheduleEvent: "Move it",
  recordAgentDraftRescheduleEvent: (meeting: string): string =>
    `Move “${meeting}” to `,
  recordAgentVerbCancelEvent: "Cancel it",
  recordAgentDraftCancelEvent: (meeting: string): string =>
    `Cancel “${meeting}”.`,
  recordAgentOriginSender: (who: string): string => `Sent by ${who}.`,
  recordAgentVerbCatchUpRoom: "Catch me up",
  recordAgentDraftCatchUpRoom: (room: string): string =>
    `Catch me up on “${room}”.`,
  recordAgentVerbFindInRoom: "Find something in it",
  recordAgentDraftFindInRoom: (room: string): string =>
    `In “${room}”, find `,
  recordAgentVerbMeetingRecord: "What happened in it",
  recordAgentDraftMeetingRecord: (meeting: string): string =>
    `What happened in “${meeting}”?`,
  recordAgentVerbMeetingMinutes: "Write the minutes",
  recordAgentDraftMeetingMinutes: (meeting: string): string =>
    `Write up the minutes of “${meeting}”.`,
  recordAgentVerbInsightChange: "How has it changed",
  recordAgentDraftInsightChange: (chart: string): string =>
    `How has “${chart}” changed since the period before?`,
  recordAgentVerbPinChart: "Pin a chart to it",
  recordAgentDraftPinChart: (board: string): string =>
    `Pin a chart to the “${board}” board that shows `,
  recordAgentVerbDraftReply: "Draft a reply",
  recordAgentDraftDraftReply: (subject: string): string =>
    `Draft a reply to “${subject}” saying `,
  recordAgentVerbThreadLookup: "Catch me up on it",
  recordAgentDraftThreadLookup: (subject: string): string =>
    `Sum up the conversation “${subject}”.`,
  recordAgentVerbCorrespondence: "What we’ve said to them",
  recordAgentDraftCorrespondence: (person: string): string =>
    `What have we said to ${person}?`,
  recordAgentVerbWriteToThem: "Write to them",
  recordAgentDraftWriteToThem: (person: string): string =>
    `Draft an email to ${person} about `,
  recordAgentVerbSiteStatus: "How it stands",
  recordAgentDraftSiteStatus: (site: string): string =>
    `How does the “${site}” website stand?`,
  recordAgentVerbSiteSeoReview: "Review it for search engines",
  recordAgentDraftSiteSeoReview: (site: string): string =>
    `Review “${site}” for search engines.`,
  recordAgentVerbSitePublish: "Publish it",
  recordAgentDraftSitePublish: (site: string): string => `Publish “${site}”.`,
  chatSearchPlaceholder: "Search messages, people, channels…",
  chatSearchClear: "Clear search",
  chatSearchNothing: "Nothing matched.",
  chatSearchFailed: "That search couldn’t be run.",
  chatProposalNotYours:
    "Only the person who asked can approve this — it would run with their access.",
  chatProposalSettled: (state: string): string =>
    state === "approved" ? "Approved and done." : `This was ${state}.`,
  chatDecideFailed: "That couldn’t be decided.",
  chatAttach: "Attach a file",
  chatShare: "Share something",
  chatShareFile: "File from Drive",
  chatShareFileHint: "A pointer, not a copy — it stays in Drive",
  chatShareMention: "Mention someone",
  chatShareMentionHint: "People and agents in this conversation",
  chatShareAsk: "Ask alo",
  chatShareAskHint: "Answers from across your workspace",
  chatInsertEmoji: "Emoji",
  chatEmojiSearch: "Search emoji",
  chatEmojiNone: "No emoji matches that.",
  chatUnstage: (name: string): string => `Remove ${name}`,
  chatAttachFailed: "That file couldn’t be shared.",
  chatOpenFile: "Open in Drive",
  chatFileTrashed: "in Drive’s trash",
  // The Drive file picker (first used by chat).
  pickerTitle: "Choose a file",
  pickerPlaces: "Where to look",
  pickerMyDrive: "My Drive",
  pickerLoading: "Loading…",
  pickerEmpty: "Nothing here yet.",
  pickerLoadFailed: "That folder couldn’t be opened.",
  pickerAttach: "Attach",
  pickerNonePicked: "No files chosen",
  pickerPicked: (count: number, max: number): string =>
    `${count} of ${max} chosen`,
  pickerPersonalNotice:
    "Files in My Drive are yours alone — people in the conversation won’t be able to open them. Use a Space to share.",
  cancel: "Cancel",
  chatMentionsYou: (count: number): string =>
    count === 1 ? "1 message mentions you" : `${count} messages mention you`,
  chatReactFailed: "That reaction couldn’t be saved.",
  chatReplies: (count: number): string =>
    count === 1 ? "1 reply" : `${count} replies`,
  chatThread: "Thread",
  chatThreadClose: "Close thread",
  chatThreadEmpty: "No replies yet — start this one.",
  chatThreadPlaceholder: "Reply…",
  chatThreadFailed: "That thread couldn’t be loaded.",
  chatArchivedNote:
    "This channel is archived. Its history stays here to read, but nothing new can be sent.",
  chatNoChannelsLead: "No conversations yet",
  chatNoChannelsHint:
    "Make a channel for a team or a topic, and everyone in it sees the same history.",
  chatNoRoomOpenLead: "Pick a conversation",
  chatNoRoomOpenHint: "Choose a channel on the left, or make a new one.",
  chatComposerLabel: "Write a message",
  chatComposerPlaceholder: (room: string): string => `Message ${room}`,
  chatLoadFailed: "Those conversations couldn’t be loaded.",
  chatSendFailed: "That message couldn’t be sent — your words are still here.",
  chatCreateFailed: "That channel couldn’t be created.",
  agentActWhatsOn: "Read your calendar",
  agentActAmIFree: "Check for a clash",
  agentActCatchUp: "Read what was said",
  agentActFindInChat: "Search conversations",
  agentActFindFile: "Search your Drive",
  agentActFindContact: "Look up a contact",
  agentFieldRoom: "Conversation",
  agentFieldLookingFor: "Looking for",
  agentProposedAction: "alo would like to do this — approve to continue.",
  agentApprove: "Approve",
  agentDiscard: "Discard",
  agentDone: "Done.",
  agentFailed: "That action couldn’t be completed.",
  // Rich approval card (ADR 0034): a title + preview per proposed action.
  agentActDraft: "New email",
  agentActReply: "Reply",
  agentActSend: "Send email",
  agentActArchive: "Archive",
  agentActTrash: "Move to Trash",
  agentActMarkRead: "Mark as read",
  agentActMarkUnread: "Mark as unread",
  agentActFlag: "Flag",
  agentActUnflag: "Remove flag",
  agentActSnooze: "Snooze",
  agentActMove: "Move to folder",
  agentActTask: "Create task",
  agentActEvent: "Add to calendar",
  // Billing tools (ADR 0035, B1.25). Each one is a draft: nothing is issued,
  // numbered or sent by approving it.
  agentActInvoiceDraft: "Draft invoice",
  agentActQuoteToInvoice: "Accept quote",
  agentActPaymentReminder: "Payment reminder",
  agentFieldCustomer: "Customer",
  agentFieldLines: "Lines",
  agentFieldQuote: "Quote",
  agentFieldInvoice: "Invoice",
  agentLineCount: (n: number): string => (n === 1 ? "1 line" : `${n} lines`),
  agentInvoiceDraftNote:
    "Creates a draft — nothing is issued, numbered or sent.",
  agentQuoteToInvoiceNote:
    "Closes the quote as accepted and raises a draft invoice.",
  agentReminderNote: "Writes a reminder into your Drafts — nothing is sent.",
  // CRM tools (ADR 0035, B2.10). A deal is named, not numbered, so each card
  // shows the title being acted on.
  agentActCreateDeal: "New deal",
  agentActMoveDeal: "Move deal",
  agentActFollowup: "Follow-up email",
  agentFieldDeal: "Deal",
  agentFieldCompany: "Company",
  agentFieldValue: "Value",
  agentFieldStage: "Stage",
  agentFieldLostReason: "Lost because",
  agentDealFromEmailNote: "Links this conversation to the new deal.",
  agentFollowupNote: "Writes the email into your Drafts — nothing is sent.",
  agentSendButton: "Send",
  agentSendCaution: "This sends the email now — it can’t be undone.",
  agentFieldTo: "To",
  agentFieldSubject: "Subject",
  agentFieldEmail: "Email",
  agentFieldReplyTo: "In reply to",
  agentFieldUntil: "Until",
  agentFieldFolder: "Folder",
  agentFieldDue: "Due",
  agentFieldWhen: "When",
  agentFieldTask: "Task",
  agentFieldEvent: "Event",
  agentNoSubject: "(no subject)",
  // Projects tools (ADR 0035, B3.10a). Logged time is a suggestion until the
  // person whose timesheet it is accepts it; the status summary only reads.
  agentActLogTime: "Log time",
  agentActProjectStatus: "Project status",
  agentFieldProject: "Project",
  agentFieldDay: "Day",
  agentFieldDuration: "Duration",
  agentLogTimeNote:
    "Suggests an entry in your timesheet — it counts once you accept it there.",
  agentProjectStatusNote: "Only reads the project — nothing is changed.",
  // The summary's own figures. The server sends numbers, never a sentence, so
  // every word a reader sees is written here.
  agentTimeLogged: (project: string): string =>
    `Suggested in your timesheet on ${project} — accept it in Projects to count it.`,
  agentStatusHours: "Hours logged",
  agentStatusBillable: (formatted: string): string => `${formatted} billable`,
  agentStatusBudget: "Budget",
  agentStatusBudgetUsed: (percent: string): string => `${percent} used`,
  agentStatusNoBudget: "No hours budget set",
  agentStatusInternal: "Internal project — no client, no budget.",
  agentStatusCustomer: "Client",
  agentStatusMilestones: "Milestones",
  agentStatusMilestonesDone: (done: number, total: number): string =>
    `${done} of ${total} reached`,
  agentStatusMilestonesLate: (late: number): string =>
    late === 1 ? "1 overdue" : `${late} overdue`,
  agentStatusNoMilestones: "None planned",
  agentStatusNext: "Next",
  agentStatusTasks: "Tasks",
  agentStatusTasksOpen: (open: number): string =>
    open === 1 ? "1 open" : `${open} open`,
  agentStatusTasksOverdue: (overdue: number): string => `${overdue} past due`,
  agentStatusLastWorked: "Last worked",
  agentStatusNeverWorked: "No hours yet",
  // The calendar draft (B3.10b). A batch of suggestions, plus what it left out
  // — the server sends reason codes, and every word for them is written here.
  agentActDraftTimesheet: "Timesheet from your calendar",
  agentDraftTimesheetNote:
    "Suggests one entry per meeting in your calendar on those days — each counts once you accept it in Projects.",
  agentDraftedCount: (count: number): string =>
    count === 1 ? "1 entry suggested" : `${count} entries suggested`,
  agentDraftedNone: "Nothing to suggest",
  agentDraftedRange: (from: string, to: string): string =>
    from === to ? from : `${from} – ${to}`,
  agentDraftedTotal: "Total",
  agentDraftedOverlap: "overlaps the one before it",
  agentDraftedOverlaps: (count: number): string =>
    count === 1
      ? "1 of them overlaps another meeting — check which was the work."
      : `${count} of them overlap other meetings — check which was the work.`,
  agentDraftedNote: (project: string): string =>
    `Suggested in your timesheet on ${project} — accept each one in Projects to count it.`,
  agentDraftedLeftOut: "Left out",
  agentDraftedReason: (reason: string): string => {
    switch (reason) {
      case "allDay":
        return "all-day — not hours worked";
      case "alreadyDrafted":
        return "already in your timesheet";
      case "noDuration":
        return "no length";
      case "tooLong":
        return "longer than a day";
      case "weekLocked":
        return "that week is submitted";
      case "limitReached":
        return "over the batch limit — ask again for the remaining days";
      case "outsideRange":
        return "starts outside those days";
      default:
        // A reason a newer server knows and this client does not: say it was
        // left out rather than pretend it was drafted.
        return "left out";
    }
  },
  // The finance agent's categorise tool (B4.14a). A suggestion is not a
  // classification, and every word on the card says so — the server sends
  // figures and reason codes only.
  agentActCategorise: "Suggest categories",
  agentCategoriseNote:
    "Looks at your own claims with no category and suggests one for each, from the categories you have used for that merchant before. Nothing is classified until you accept it.",
  agentCategoriseFieldPeriod: "Claims from",
  agentCategoriseSuggested: (count: number): string =>
    count === 1 ? "1 suggestion" : `${count} suggestions`,
  agentCategoriseNone: "Nothing to suggest",
  agentCategoriseConsidered: (count: number): string =>
    count === 1 ? "1 claim looked at" : `${count} claims looked at`,
  agentCategoriseEvidence: (times: number): string =>
    times === 1
      ? "booked here once before"
      : `booked here ${times} times before`,
  agentCategoriseAccept: "Accept",
  agentCategoriseDecline: "No",
  agentCategoriseAccepted: "Accepted",
  agentCategoriseDeclined: "Declined",
  agentCategoriseLeftOut: "Left out",
  agentCategoriseNoMerchant: "No merchant",
  agentCategoriseFooter:
    "Each suggestion waits for you — nothing is booked, reported or returned until you accept it.",
  agentCategoriseFailed: "That could not be answered — try again from Finance.",
  agentCategoriseReason: (reason: string): string => {
    switch (reason) {
      case "noMerchant":
        return "no merchant to recognise it by";
      case "noHistory":
        return "you have never classified this merchant";
      case "alreadyProposed":
        return "already has a suggestion";
      case "declined":
        return "you said no to a suggestion here";
      default:
        // A reason a newer server knows and this client does not: say it was
        // left out rather than pretend something was suggested.
        return "left out";
    }
  },
  // The finance agent's two answers (B4.14b). Both only read: the words say so
  // more than once, because a person told "your VAT" by a machine will assume
  // something was filed unless the card says it was not.
  agentActVatSummary: "VAT figures",
  agentVatSummaryNote:
    "Reads the VAT your books carry for those days — tax charged, tax paid, and the difference. Nothing is filed and nothing is changed.",
  agentVatFieldPeriod: "Period",
  agentVatCharged: "Charged on sales",
  agentVatPaid: "Paid on purchases",
  agentVatOwed: "You owe",
  agentVatRefund: "You are owed back",
  agentVatBaseSales: "Turnover",
  agentVatBaseCosts: "Costs",
  agentVatUnrated: "On no rate",
  agentVatRateRow: (rate: string, base: string): string => `${rate} of ${base}`,
  agentVatNothing: "Nothing in these days",
  agentVatFooter:
    "Figures for a return, not a return — filing still happens in your national portal.",
  // The books-check. Every word here is a question, never a verdict: the tool
  // has no score, and the card must not invent one.
  agentActFlagAnomalies: "Check the books",
  agentAnomalyNote:
    "Reads your journal for those days and names what is worth a second look, with the entries behind each one. It writes nothing and marks nothing as reviewed.",
  agentAnomalyFieldPeriod: "Books from",
  agentAnomalyFound: (count: number): string =>
    count === 1 ? "1 worth a look" : `${count} worth a look`,
  agentAnomalyNone: "Nothing stood out",
  agentAnomalyScanned: (count: number): string =>
    count === 1 ? "1 entry read" : `${count} entries read`,
  agentAnomalyShown: (shown: number, found: number): string =>
    `showing ${shown} of ${found}`,
  agentAnomalyTruncated:
    "These days hold more entries than one check reads — ask again for a shorter period to see the rest.",
  agentAnomalyNotComparable: (count: number): string =>
    count === 1
      ? "1 entry names no customer or supplier, so it could not be compared"
      : `${count} entries name no customer or supplier, so they could not be compared`,
  agentAnomalyKind: (kind: string): string => {
    switch (kind) {
      case "duplicate":
        return "Booked twice in a week";
      case "unusualAmount":
        return "Unlike the rest of this account";
      case "missingRecurring":
        return "A month with nothing in it";
      default:
        // A kind a newer server knows and this client does not: still a
        // question, never nothing.
        return "Worth a look";
    }
  },
  agentAnomalyTypical: (amount: string): string => `usually ${amount}`,
  agentAnomalyMissingMonth: (month: string): string => `nothing in ${month}`,
  agentAnomalyEvidence: "The entries behind it",
  agentAnomalyFooter:
    "Nothing was changed and nothing was marked as reviewed — each of these is a question about entries, and the answer to one is a correcting entry.",
  // The inventory agent (ADR 0035, B5.10). Every word here keeps a draft a
  // draft: the card must never let a reader believe a supplier has been
  // contacted, because a purchase order is a document that goes to another
  // company and there is no unsending one.
  agentActReorderProposals: "Draft the reorders",
  agentReorderNote:
    "Looks at everything you are under your own minimum on and writes one draft purchase order per supplier. Nothing is sent — each draft waits in your purchase orders for you to check and send.",
  agentActStockAnswer: "Check stock",
  agentStockAnswerNote:
    "Reads where one product stands right now: on your shelves, on order, promised to customers. It changes nothing and reserves nothing.",
  agentFieldSupplier: "Supplier",
  agentFieldLocation: "Place",
  agentFieldProduct: "Product",
  agentReorderEverySupplier: "Every supplier",
  agentReorderEverywhere: "Everywhere",
  agentReorderShortages: (count: number): string =>
    count === 1 ? "1 under minimum" : `${count} under minimum`,
  agentReorderNothingShort: "Nothing is under its minimum",
  agentReorderDrafted: (count: number): string =>
    count === 1 ? "1 draft order" : `${count} draft orders`,
  agentReorderLines: (count: number): string =>
    count === 1 ? "1 line" : `${count} lines`,
  agentReorderLeftOut: "Ordered nothing for",
  agentReorderReason: (reason: string): string => {
    switch (reason) {
      case "noSupplier":
        return "nobody has quoted you for it";
      case "nothingToBuy":
        return "the rule asks for nothing";
      default:
        // A reason a newer server knows and this client does not: still
        // visibly left out, never silently dropped.
        return "left out";
    }
  },
  agentReorderNeeded: (qty: string, unit: string): string =>
    unit === "" ? `${qty} needed` : `${qty} ${unit} needed`,
  agentReorderFooter:
    "These are drafts. No supplier has been contacted and no order number has been drawn — open one in Inventory to check it and send it.",
  agentStockOnHand: "On the shelves",
  agentStockOnOrder: "On order",
  agentStockCommitted: "Promised out",
  agentStockAvailable: "That leaves",
  agentStockNoShelf: "A service — nothing is stocked",
  agentStockNowhere: "None anywhere",
  agentStockWatched: "Kept at",
  agentStockMinimum: (min: string, target: string): string =>
    `minimum ${min}, back up to ${target}`,
  agentStockBelowMinimum: "under minimum",
  agentStockFooter:
    "Figures as they stand right now. Nothing was ordered and nothing was set aside.",
  // The HR agent's one read (B6.09). Every string here is written so that the
  // card says names and days and nothing else: sickness, holiday and unpaid
  // leave are indistinguishable in what the server sends, by design, and the
  // footer tells the reader that rather than leaving them to wonder.
  agentActWhoIsOff: "See who is off",
  agentWhoIsOffNote:
    "Reads the team absence view everybody here already sees: who is away, and on which days. It changes nothing, books nothing and tells nobody.",
  agentWhoIsOffAway: "Away",
  agentWhoIsOffNobody: "Nobody",
  agentWhoIsOffCount: (count: number): string =>
    count === 1 ? "1 person" : `${count} people`,
  agentWhoIsOffDays: (count: number): string =>
    count === 1 ? "1 day" : `${count} days`,
  agentWhoIsOffFooter:
    "Names and days only — approved leave never says why somebody is away. Anyone not listed may still be out for a reason this does not cover.",
  searchKind: (kind: string): string =>
    kind === "task"
      ? "Task"
      : kind === "message"
        ? "Email"
        : kind === "folder"
          ? "Folder"
          : kind === "doc"
            ? "Doc"
            : kind === "base"
              ? "Base"
              : "File",
  driveNewBase: "New base",
  driveNewBasePrompt: "Name the new base",
  baseNewRow: "New row",
  baseAddField: "Add field",
  baseFieldName: "Field name",
  baseNewTable: "New table",
  baseTypeText: "Text",
  baseTypeNumber: "Number",
  baseTypeDate: "Date",
  baseTypeCheckbox: "Checkbox",
  baseTypeSelect: "Select",
  baseTypeMultiselect: "Multi-select",
  baseTypePerson: "Person",
  baseTypeLink: "Link to table",
  baseViewGrid: "Grid",
  baseViewBoard: "Board",
  baseViewCalendar: "Calendar",
  baseViewGallery: "Gallery",
  baseAddView: "Add view",
  baseGroupBy: "Group by…",
  baseByDate: "By date…",
  baseChoicesPlaceholder: "Choices, comma-separated",
  baseLinkTarget: "Linked table…",
  baseUncategorised: "Uncategorised",
  baseBoardNeedsSelect:
    "Add a board view grouped by a Select field to use this.",
  baseCalendarNeedsDate:
    "Add a calendar view based on a Date field to use this.",
  baseBoardEmptyTitle: "Group records into a board",
  baseCalendarEmptyTitle: "Put records on a calendar",
  baseBoardEmptyBody:
    "Boards group records by a Select field. Add a ready-to-use Status field to continue.",
  baseCalendarEmptyBody:
    "Calendars place records by a Date field. Add one to continue.",
  baseAddStatusField: "Add Status field",
  baseAddDateField: "Add Date field",
  baseStatusField: "Status",
  baseDateField: "Date",
  baseStatusTodo: "To do",
  baseStatusInProgress: "In progress",
  baseStatusDone: "Done",
  baseCalendarPreviousMonth: "Previous month",
  baseCalendarNextMonth: "Next month",
  baseCalendarAddOnDate: (date: string): string => `Add a record on ${date}`,
  baseLoading: "Loading your base…",
  baseLoadFailedTitle: "This base didn’t load",
  baseEmptyTitle: "Start with your first table",
  baseEmptyBody:
    "Tables keep related records together. Create one to start adding fields and records.",
  baseDefaultTableName: (number: number): string => `Table ${number}`,
  baseView: "View",
  baseSaveChanges: "Save changes",
  baseUntitledRecord: "Untitled",
  basePersonPlaceholder: "email@…",
  baseNoChoices: "No choices yet — add some on the field.",
  baseLink: "Link",
  baseLinkNoTable: "No linked table set.",
  baseLinkNoRecords: "The linked table has no records yet.",

  // alo Billing (ADR 0035, wave B1) — customers and the price list. Wording
  // note: the module speaks about documents ("raise an invoice"), not about
  // rows, and never states a validation rule the server owns — a refusal is
  // shown in the server's own words so the two can never disagree.
  billingCustomers: "Customers",
  billingProducts: "Price list",
  billingSearchCustomers: "Search customers…",
  billingSearchProducts: "Search the price list…",
  billingShowArchived: "Show archived",
  billingArchived: "Archived",
  billingArchive: "Archive",
  billingRestore: "Restore",
  billingNewCustomer: "New customer",
  billingNewProduct: "New item",
  billingEditCustomer: "Edit customer",
  billingEditProduct: "Edit item",
  billingCustomerSubtitle: "Who your invoices are made out to.",
  billingProductSubtitle: "An item you can pick when you raise a document.",
  billingArchiveCustomerConfirm: (name: string) =>
    `Archive ${name}? They disappear from the pickers; every document already raised still names them.`,
  billingArchiveProductConfirm: (name: string) =>
    `Archive ${name}? It disappears from the pickers; documents already raised keep the price they were raised at.`,
  billingCreate: "Create",
  billingSave: "Save",
  billingCancel: "Cancel",
  billingLoadFailed:
    "Could not load this list. Check your connection and try again.",
  billingLoading: "Loading billing data…",
  billingPaginationLabel: "Billing list pages",
  billingPaginationPrevious: "Previous page",
  billingPaginationNext: "Next page",
  billingPaginationRange: (first: number, last: number, total: number) => `${first}–${last} of ${total}`,
  billingPaginationPage: (page: number, total: number) => `Page ${page} of ${total}`,
  billingSaveFailed: "Could not save. Check your connection and try again.",
  billingNoMatches: "Nothing matches that search.",
  billingNoCustomersTitle: "No customers yet",
  billingNoCustomersBody:
    "A customer carries the address, VAT id and payment terms every invoice you raise for them starts from.",
  billingGetStarted: "Get started in 3 simple steps",
  billingStepCustomerTitle: "Add your first customer",
  billingStepCustomerBody:
    "Create a customer profile with their billing details.",
  billingStepInvoiceTitle: "Create your first invoice",
  billingStepInvoiceBody: "Add items, set payment terms and issue it.",
  billingStepPaidTitle: "Get paid faster",
  billingStepPaidBody: "Record payments and keep track of your cash flow.",
  billingNoProductsTitle: "Your price list is empty",
  billingNoProductsBody:
    "Add the things you sell once, then pick them when you raise a quote or an invoice.",
  billingColName: "Name",
  billingColLocation: "Location",
  billingColVatId: "VAT id",
  billingColEmail: "Email",
  billingColTerms: "Payment terms",
  billingColCurrency: "Currency",
  billingColUnit: "Unit",
  billingColUnitPrice: "Unit price",
  billingColVatRate: "VAT rate",
  billingColActions: "Actions",
  billingTermsDays: (days: number) => `${days} days`,
  billingFieldName: "Name",
  billingFieldEmail: "Invoice email",
  billingFieldAddress: "Address",
  billingFieldAddress2: "Address, second line",
  billingFieldPostalCode: "Postal code",
  billingFieldCity: "City",
  billingFieldCountry: "Country",
  billingFieldVatId: "VAT id",
  billingFieldTerms: "Payment terms (days)",
  billingFieldCurrency: "Currency",
  billingFieldUnit: "Unit",
  billingFieldUnitPrice: "Unit price",
  billingFieldVatRate: "VAT rate (%)",
  billingEmailPlaceholder: "billing@example.com",
  billingAddressPlaceholder: "Street and number",
  billingCountryPlaceholder: "BE",
  billingCountryHint: "Two-letter country code.",
  billingCurrencyPlaceholder: "EUR",
  billingVatIdPlaceholder: "BE0123456789",
  billingVatIdHint: "Leave empty for a private customer.",
  billingTermsPlaceholder: "30",
  billingTermsHint: "Days from issue to due date.",
  billingUnitPlaceholder: "hour",
  billingUnitHint: "What one of it is called. Leave empty for a flat item.",
  billingAmountPlaceholder: "0.00",
  billingPriceHint: "Excluding VAT.",
  billingRatePlaceholder: "21",
  billingRateHint: "0 for an exempt item.",
  billingNotAnAmount: "Enter an amount like 1250.00.",
  billingNotARate: "Enter a rate like 21.",

  // Invoices (B1.14): the list, and the draft editor. Every figure a user
  // reads here is the server's — the wording never promises a total the
  // browser worked out, and says so plainly when a figure is one edit behind.
  billingInvoices: "Invoices",
  billingNewInvoice: "New invoice",
  billingSearchInvoices: "Search by number, customer or reference…",
  billingFilterStatus: "Show",
  billingFilterAll: "All documents",
  billingStatusDraft: "Draft",
  billingStatusIssued: "Issued",
  billingStatusPaid: "Paid",
  billingStatusVoid: "Void",
  billingStatusOverdue: "Overdue",
  billingCreditNote: "Credit note",
  billingCreditNotes: "Credit notes",
  billingNoInvoicesTitle: "No invoices yet",
  billingNoInvoicesBody:
    "Raise a draft for a customer, add what you are billing them for, and issue it when it is right.",
  billingColNumber: "Number",
  billingColCustomer: "Customer",
  // "Issue date", not "Issued": a column header that reads the same as the
  // status chip below it makes a list of documents ambiguous at a glance.
  billingColIssueDate: "Issue date",
  billingColDueDate: "Due date",
  billingColStatus: "Status",
  billingColTotal: "Total",
  billingColDescription: "Description",
  billingColQty: "Quantity",
  billingColNet: "Net",
  /** A draft has no number and no dates yet — it has not consumed one. */
  billingNotNumbered: "—",
  billingNoDate: "—",
  billingUnknownCustomer: "Unknown customer",
  billingDraftInvoice: "Draft invoice",
  billingBackToInvoices: "All invoices",
  billingBackToProject: (name: string) => `Back to ${name}`,
  billingInvoiceGone: "This document no longer exists.",
  billingFieldCustomer: "Customer",
  billingChooseCustomer: "Choose a customer…",
  billingCustomerFixedHint:
    "Their currency and payment terms are copied onto the document.",
  billingFieldReference: "Customer reference (optional)",
  billingReferencePlaceholder: "For example, PO-1234",
  billingReferenceHint:
    "Enter a PO or RFQ supplied by the customer. Alo assigns this document its own unique number automatically when you finalize it.",
  billingFieldNote: "Note",
  billingNotePlaceholder: "Anything the customer should read on the document.",
  billingNoteHint: "Printed under the lines.",
  billingFieldIssueDate: "Issue date",
  billingFieldDueDate: "Due date",
  billingCreateDraft: "Create draft",
  billingCreateDraftHint:
    "The draft is raised first; then you add what you are billing for.",
  billingLines: "Lines",
  billingAddLine: "Add line",
  billingRemoveLine: "Remove this line",
  billingNoLines: "Nothing on this document yet.",
  billingPickProduct: "From the price list…",
  billingDescriptionPlaceholder: "What you are billing for",
  billingQtyPlaceholder: "1",
  billingLineNeedsDescription:
    "A line needs a description before the draft can save.",
  billingNotAQuantity: "Enter a quantity like 1.5.",
  billingTotalsNet: "Net",
  billingTotalsGross: "Total",
  billingVatAtRate: (rate: string) => `VAT at ${rate}`,
  billingTotalsStale:
    "These are the last figures the server sent; they update when the draft saves.",
  billingSaving: "Saving…",
  billingSaved: "Saved",
  billingUnsaved: "Not saved yet",
  billingSaveNotDone: "Could not save",
  billingSaveNow: "Try again",
  billingDeleteDraft: "Delete draft",
  billingDeleteDraftConfirm:
    "Delete this draft? It carries no number, so nothing is left behind — and nothing can be recovered.",
  billingFrozenNotice:
    "This document carries a number and can no longer be changed. Correct it with a credit note.",

  // Lifecycle (B1.15). Every one of these actions is irreversible on a legal
  // document, so the confirmation says what it will DO — spends a number,
  // freezes the prices, closes the offer — rather than asking whether the
  // person is sure. None of them promises an email: nothing is sent to anyone
  // until B1.18.
  billingActionFailed:
    "That did not go through. Check your connection and try again.",
  billingActionsWaitForSave:
    "These wait until your last change has been saved.",
  billingIssue: "Issue & prepare email",
  billingIssueTitle: "Issue and prepare the customer email?",
  billingIssueConfirm:
    "This takes the next invoice number, dates and freezes the document, then opens a complete customer email with the PDF attached. You review it in Mail before sending.",
  billingPrepareInvoiceEmail: "Prepare customer email",
  billingPrepareInvoiceEmailTitle: "Prepare this invoice for the customer?",
  billingPrepareInvoiceEmailConfirm:
    "A complete email addressed to the customer will open in Mail with this invoice attached. Nothing is sent until you press Send.",
  billingVoid: "Void",
  billingVoidTitle: "Void this invoice?",
  billingVoidConfirm:
    "A void invoice keeps its number and stays readable, but is worth nothing. Void one nobody has seen; if the customer already holds this document, raise a credit note instead.",
  billingVoidNotice:
    "This invoice has been voided. It keeps its number and is worth nothing.",
  billingCreditNoteAction: "Credit note",
  billingCreditNoteTitle: "Raise a credit note?",
  billingCreditNoteConfirm:
    "This raises a draft credit note mirroring every line of this invoice. Edit it down for a partial credit, then issue it like any other document.",
  billingCreditsInvoice: "The invoice this credits",
  billingFromQuote: "The quote this came from",

  // Payments (B1.19): the money received against an invoice. Every figure here
  // is the server's — the wording never promises a total the browser summed —
  // and "partly paid" is deliberately never called a status: the document is
  // still issued, still owed, and still late when its date passes.
  billingPayments: "Payments",
  billingRecordPayment: "Record payment",
  billingRecordPaymentHint:
    "Money that has arrived. It is not sent anywhere — this only records what your bank already shows.",
  billingRemovePayment: "Remove",
  billingNoPayments: "Nothing has been received against this invoice yet.",
  billingPaidToDate: "Received",
  billingOutstanding: "Still owed",
  billingOverpaidNote:
    "More has been received than this invoice is worth. The difference is yours to refund or to credit against the next one.",
  billingPaymentUnpaid: "Unpaid",
  billingPaymentPartiallyPaid: "Partly paid",
  billingPaymentPaid: "Settled",
  billingColPaidOn: "Received on",
  billingColMethod: "How",
  billingColPaymentReference: "Bank reference",
  billingColAmount: "Amount",
  billingFieldAmount: (currency: string) => `Amount (${currency})`,
  billingFieldAmountHint:
    "What actually arrived, which may be less than the invoice.",
  billingFieldPaidOn: "Received on",
  billingFieldPaidOnHint:
    "The day your bank shows it. Leave it empty for today.",
  billingFieldMethod: "How it arrived",
  billingFieldMethodHint: "Free text — whatever your bookkeeping calls it.",
  billingMethodPlaceholder: "Bank transfer",
  billingFieldPaymentReference: "Bank reference",
  billingFieldPaymentRefHint:
    "The reference on the statement line, so it can be matched later.",
  billingFilterOverdue: "Overdue",
  billingColOutstanding: "Still owed",

  // The VAT summary of a period (B1.20): the figures a return is copied from.
  // The wording says plainly which documents are counted and which are not,
  // because a person is legally answerable for what they copy off this screen.
  billingReports: "VAT report",
  billingReportFrom: "From",
  billingReportTo: "To",
  billingReportShow: "Show",
  billingReportThisQuarter: "This quarter",
  billingReportLastQuarter: "Last quarter",
  billingReportDownloadCsv: "Download CSV",
  billingReportDownloadFailed: "The file could not be prepared. Try again.",
  billingReportBasis: (from: string, to: string) =>
    `Issued and paid documents dated ${from} to ${to}. Credit notes are subtracted; drafts and cancelled documents are not counted.`,
  billingReportColVat: "VAT",
  billingReportTotal: "Total",
  billingReportGross: "Including VAT",
  billingReportOverview: "Filing overview",
  billingReportTaxableNet: "Taxable net",
  billingReportVatDue: "VAT due",
  billingReportGrossBilled: "Gross billed",
  billingReportDocuments: "Documents",
  billingReportCurrencyDetail: "Currency detail",
  billingReportCaption: (currency: string) => `VAT summary in ${currency}`,
  billingReportCounts: (invoices: number, creditNotes: number) =>
    `From ${invoices} invoices and ${creditNotes} credit notes.`,
  billingReportEmptyTitle: "Nothing was issued in this period",
  billingReportEmptyBody:
    "A document counts from the day it was issued. Pick a different period, or issue the drafts that belong in this one.",

  // Quotes (B1.15): the same document as an invoice until somebody says yes,
  // and deliberately the same words wherever the two screens agree.
  billingQuotes: "Quotes",
  billingQuotation: "Quotation",
  billingPreparedFor: "Prepared exclusively for this customer",
  billingIncludingVat: "Including VAT",
  billingQuoteTemplate: "Quote template",
  billingQuoteStartFrom: "Start from a template",
  billingQuoteTemplateHint:
    "Use your live price list to prepare a useful starting point.",
  billingQuoteTemplateBlank: "Blank quote",
  billingQuoteTemplateBlankDescription: "Start with an empty pricing table.",
  billingQuoteTemplateServices: "Professional services",
  billingQuoteTemplateServicesDescription:
    "A focused offer with two core services.",
  billingQuoteTemplateProject: "Project delivery",
  billingQuoteTemplateProjectDescription:
    "A broader scope with three delivery items.",
  billingQuoteTemplateRetainer: "Ongoing partnership",
  billingQuoteTemplateRetainerDescription:
    "Start from a recurring monthly service.",
  quoteStudioTemplateServicesHeading: "Services selected for you",
  quoteStudioTemplateServicesIntroduction:
    "A clear overview of the services, deliverables and investment proposed for your business.",
  quoteStudioTemplateServicesTable: "Services and fees",
  quoteStudioTemplateProjectHeading: "Project proposal",
  quoteStudioTemplateProjectIntroduction:
    "This proposal brings the project scope, delivery approach and commercial terms together in one place.",
  quoteStudioTemplateProjectDiscovery: "Discovery and alignment",
  quoteStudioTemplateProjectDelivery: "Delivery and review",
  quoteStudioTemplateProjectHandover: "Launch and handover",
  quoteStudioTemplateProjectTable: "Project investment",
  quoteStudioTemplateRetainerHeading: "Monthly partnership",
  quoteStudioTemplateRetainerIntroduction:
    "Ongoing support with a predictable monthly investment and a clear working rhythm.",
  quoteStudioTemplateRetainerTable: "Monthly services",
  quoteStudioTemplateRetainerReporting: "Regular progress reporting",
  quoteStudioTemplateRetainerSupport: "Priority support and planning",
  billingQuoteIncludedItems: (count: number) =>
    `${count} item${count === 1 ? "" : "s"}`,
  billingQuoteIncludedTitle: "Items ready to add",
  billingQuoteIncludedHelp:
    "Review what this template will add. You can adjust quantities, prices and descriptions in the editor.",
  billingQuoteRemoveIncludedItem: (name: string) => `Remove ${name}`,
  billingQuoteAddFromPriceList: "Add items",
  billingQuoteSearchPriceList: "Search the price list",
  billingQuoteAllItemsIncluded:
    "Every active price-list item is already included.",
  billingQuoteNoMatchingItems: "No price-list items match that search.",
  billingQuotePerItem: "each",
  billingQuoteContinueToEditor: "Continue to editor",
  billingNewQuote: "New quote",
  billingSearchQuotes: "Search by number, customer or reference…",
  billingNoQuotesTitle: "No quotes yet",
  billingNoQuotesBody:
    "Offer a customer a price. When they accept, the quote becomes a draft invoice with the same lines.",
  billingQuoteStatusSent: "Finalized",
  billingQuoteStatusAccepted: "Accepted",
  billingQuoteStatusDeclined: "Declined",
  billingQuoteStatusExpired: "Expired",
  /** The computed flag: the validity date has passed. Worded apart from the
   *  "Expired" status, which is somebody's decision to stop chasing it. */
  billingQuoteLapsed: "Past its date",
  // "Sent on", not "Sent", for the same reason the invoice list says "Issue
  // date": a column header that reads the same as the status chip under it
  // makes a list of documents ambiguous at a glance.
  billingColSentDate: "Finalized on",
  billingColValidUntil: "Valid until",
  billingColCreated: "Created",
  billingColLastEdited: "Last edited",
  billingDraftQuote: "Draft quote",
  billingBackToQuotes: "All quotes",
  billingQuoteGone: "This quote no longer exists.",
  billingQuoteCustomerHint: "Their currency is copied onto the offer.",
  billingCreateQuoteHint:
    "The draft is raised first; then you add what you are offering.",
  billingFieldSentDate: "Finalized on",
  billingFieldValidUntil: "Valid until",
  billingValidForDays: (days: number) =>
    `Valid for ${days} days from finalization.`,
  billingDeleteQuoteDraft: "Delete draft",
  billingDeleteQuoteDraftConfirm:
    "Delete this draft? It carries no number and was never made to anybody — and nothing can be recovered.",
  billingQuoteSentNotice: "Finalized in alo and ready to share with the customer.",
  billingQuoteClosedNotice:
    "This offer is closed and can no longer be changed.",
  billingSendQuote: "Finalize & prepare email",
  billingSendQuoteTitle: "Finalize and prepare the customer email?",
  billingSendQuoteConfirm:
    "This assigns the next quote number, records the date and locks the prices, then opens a complete customer email with the PDF attached. You review it in Mail before sending.",
  billingPrepareQuoteEmail: "Prepare customer email",
  billingPrepareQuoteEmailTitle: "Prepare this quotation for the customer?",
  billingPrepareQuoteEmailConfirm:
    "A complete email addressed to the customer will open in Mail with this quotation attached. Nothing is sent until you press Send.",
  billingAcceptQuote: "Accept quote",
  billingAcceptQuoteTitle: "The customer accepted?",
  billingAcceptQuoteConfirm:
    "This closes the offer and raises a draft invoice with the same lines at the same prices. Nothing is issued yet — you will land on the draft.",
  billingDeclineQuote: "Decline quote",
  billingDeclineQuoteTitle: "The customer declined?",
  billingDeclineQuoteConfirm:
    "The offer closes for good and stays readable. A change of mind is a new quote, not a reopened one.",
  billingExpireQuote: "Mark as expired",
  billingExpireQuoteTitle: "Stop chasing this offer?",
  billingExpireQuoteConfirm:
    "The offer closes as expired, with today as the day you stopped chasing it. It cannot be answered afterwards.",
  billingQuoteInvoice: "The invoice this became",

  // Printing, and the issuer identity every printed document carries (B1.16).
  // The document itself is rendered by the server and speaks its own language
  // table (`billing_print.rs`); these are the words around it.
  billingPrint: "Print",
  billingPrintUnsaved:
    "This prints the saved document, so it waits for your last change.",
  billingPrintFailed:
    "The document could not be prepared for printing. Try again.",
  billingSettings: "Your details",
  billingSettingsIntro:
    "This is who your invoices, credit notes and quotes are from: the name and numbers at the top, and the account the money goes to.",
  billingSettingsFirstRun:
    "Fill this in before you issue anything. It is what appears at the top of every document you print, and where your customers are asked to pay.",
  billingSettingsIdentity: "Who you invoice as",
  billingSettingsContact: "How customers reach you",
  billingSettingsBank: "Where the money goes",
  billingSettingsFooter: "The line under the totals",
  billingSettingsSaved:
    "Saved. Every document you print from now on carries this.",
  billingSettingsLoadFailed: "Your billing details could not be loaded.",
  billingFieldLegalName: "Legal name",
  billingLegalNameHint: "The name you trade and invoice under, as registered.",
  billingIssuerVatIdHint:
    "Leave empty if you are not VAT-registered. State your country first.",
  billingFieldRegistrationNo: "Company number",
  billingRegistrationHint:
    "As your register prints it — KVK, SIREN, HRB, Companies House.",
  billingFieldPhone: "Phone",
  billingFieldWebsite: "Website",
  billingFieldIban: "IBAN",
  billingIbanHint:
    "Checked against your country's length and its check digits before it is saved.",
  billingIbanPlaceholder: "BE68 5390 0754 7034",
  billingFieldBic: "BIC",
  billingBicPlaceholder: "KREDBEBB",
  billingBicHint: "Your bank's international BIC or SWIFT code.",
  billingFieldBankName: "Bank",
  billingFieldAccountHolder: "Account holder",
  billingAccountHolderHint: "Only if the account is not in your legal name.",
  billingFieldFooterNote: "Footer note",
  billingFooterNoteHint:
    "Printed under the totals of every document — retention of title, late-payment terms, a thank-you.",

  // Multi-currency (B1.21). The wording is careful about two things a person is
  // legally answerable for: which currency the books are kept in, and that a
  // converted total is only complete if every document in it could be converted.
  billingSettingsAccounting: "The currency you keep books in",
  billingFieldBaseCurrency: "Accounting currency",
  billingBaseCurrencyHint:
    "You can invoice in any currency. This is the one your VAT return is filed in, and the one the VAT on a foreign-currency invoice is also printed in.",
  billingFxRates: "Exchange rates",
  billingFxIntro:
    "Invoicing in another currency needs the published rate of the day you issue on. Rates are yours: nothing is fetched for you, so what your books are converted at is a file you chose.",
  billingFxColDate: "Published",
  billingFxColRate: "Rate per euro",
  billingFxColSource: "From",
  billingFxSourceEcb: "Reference file",
  billingFxSourceManual: "Entered by hand",
  billingFxAdd: "Add a rate",
  billingFxAddSaved: (currency: string, date: string) =>
    `Saved the ${currency} rate for ${date}.`,
  billingFxRateHint:
    "As published: units of this currency for one euro, written 1.1626.",
  billingFxImport: "Import a rate file",
  billingFxImportHint:
    "Paste the European Central Bank's eurofxref CSV, or any file in that shape. A file with one bad value changes nothing.",
  billingFxImportRun: "Import",
  billingFxImported: (rates: number, days: number) =>
    `Imported ${rates} rates over ${days} days.`,
  billingFxEmpty:
    "No rates yet. You only need them if you invoice in another currency.",
  billingFxLoadFailed: "The exchange rates could not be loaded.",
  billingDocumentFx: (rate: string, day: string) =>
    `Converted at ${rate}, the reference rate published on ${day}.`,
  billingVatIn: (currency: string) => `VAT in ${currency}`,
  billingReportBaseCaption: (currency: string) => `The period in ${currency}`,
  billingReportBaseIntro: (currency: string) =>
    `Every document above, converted at the rate frozen on it when it was issued. This is what a return in ${currency} is filed from.`,
  billingReportUnconverted: (count: number) =>
    count === 1
      ? "1 document is not in these figures: no exchange rate was stored for it. Check it before filing."
      : `${count} documents are not in these figures: no exchange rate was stored for them. Check them before filing.`,

  // Chasing late money (B1.26). The wording is careful about one thing above
  // all: this writes a letter, it does not send one. A product that emailed a
  // customer the moment somebody clicked "Remind" would be a product people
  // stop clicking in, so the notice says where the letter went and who sends
  // it. The figures in it are the server's own.
  billingRemind: "Remind",
  billingRemindHint:
    "Write a payment reminder to this customer, and leave it in your Drafts.",
  billingReminderDrafted: (
    invoice: string,
    outstanding: string,
    days: number,
  ) =>
    days === 1
      ? `A reminder for ${invoice} — ${outstanding} still owed, 1 day past its date — is waiting in your Drafts. Nothing has been sent: read it, change what you like, and send it yourself.`
      : `A reminder for ${invoice} — ${outstanding} still owed, ${days} days past its date — is waiting in your Drafts. Nothing has been sent: read it, change what you like, and send it yourself.`,
  billingReminderFailed:
    "The reminder could not be written. Check your connection and try again.",
  billingNothingOverdue:
    "Nothing is overdue. Every issued invoice is either settled or still in date.",

  // Recurring invoices (B2.11). The words here carry one promise above all:
  // this raises DRAFTS. A product that issued numbered invoices on a timer
  // without anyone reading them would be a product nobody trusts with their
  // ledger, so every string that mentions a run says what appears and where.
  billingRecurring: "Recurring",
  billingRecurringTitle: "Recurring invoices",
  billingRecurringChip: "Recurring",
  billingRecurringChipHint: "A recurring invoice raised this draft.",
  billingNoSchedulesTitle: "No recurring invoices yet",
  billingNoSchedulesBody:
    "Set one up for anything you bill on a rhythm — a retainer, a subscription, a hosting fee. Each time it comes due, alo raises a draft for you to check and issue.",
  billingNewSchedule: "New recurring invoice",
  billingScheduleFrom: "Repeat this invoice",
  billingScheduleFromHint:
    "Set up a recurring invoice that bills these lines again on a rhythm. Each occurrence appears as a draft — nothing is ever issued for you.",
  billingScheduleName: "Name",
  billingScheduleNameHint:
    "What you call this arrangement. Never printed on the invoice.",
  billingScheduleCadence: "Bills",
  billingCadenceWeekly: "Every week",
  billingCadenceMonthly: "Every month",
  billingCadenceQuarterly: "Every quarter",
  billingCadenceYearly: "Every year",
  billingScheduleStart: "First on",
  billingScheduleEnd: "Until",
  billingScheduleEndNever: "No end date",
  billingScheduleNext: "Next",
  billingScheduleLast: "Last raised",
  billingScheduleRaised: "Raised",
  billingScheduleEach: "Each time",
  billingScheduleStatusActive: "Running",
  billingScheduleStatusPaused: "Paused",
  billingScheduleStatusEnded: "Finished",
  billingScheduleStatusDue: "Due",
  billingSchedulePause: "Pause",
  billingScheduleResume: "Resume",
  billingScheduleDelete: "Delete",
  billingScheduleDeleteTitle: "Delete this recurring invoice?",
  billingScheduleDeleteMessage:
    "It will stop billing and disappear from this list. Only an arrangement that has never raised a draft can be deleted — pause one that has.",
  billingScheduleRunDue: "Raise what is due",
  billingScheduleRunHint:
    "alo does this on its own every hour. This is only for when you would rather not wait.",
  billingScheduleRunNone:
    "Nothing was due. Every recurring invoice is up to date.",
  billingScheduleRunDrafted: (count: number) =>
    count === 1
      ? "1 draft was raised and is waiting in your invoices. Nothing has been issued: read it, change what you like, and issue it yourself."
      : `${count} drafts were raised and are waiting in your invoices. Nothing has been issued: read them, change what you like, and issue them yourself.`,
  billingScheduleSaved: (name: string) =>
    `“${name}” is set up. Each time it comes due, alo will raise a draft for you to check.`,
  billingScheduleAnchorHint: (day: number) =>
    day > 28
      ? `Anchored to day ${day}: in a shorter month it bills on the last day, and on day ${day} again in the next long one.`
      : `Anchored to day ${day} of the month.`,

  // CRM (alo CRM, ADR 0035, wave B2). The words of a sales record: a board of
  // opportunities, what each is worth, what was said about it, what happens
  // next, and the conversation it came from.
  moduleCrm: "Sales",
  crmBoard: "Board",
  crmList: "List",
  crmPipeline: "Pipeline",
  crmDeal: "Deal",
  crmStage: "Stage",
  crmStageArchived: "Archived column",
  crmLoadFailed: "Your deals could not be loaded.",
  crmSaveFailed: "The change could not be saved.",
  crmDeleteFailed: "That could not be removed.",
  crmSuggestFailed: "No conversations could be suggested just now.",
  crmNoBoardTitle: "No pipeline yet",
  crmNoBoardBody:
    "Every board you had has been archived. Restore one to start working deals again.",
  crmNoDealsTitle: "No deals yet",
  crmNoDealsBody:
    "Raise the first opportunity and move it across the board as it progresses.",
  crmNoMatches: "No deal matches what you typed.",

  // The deal form
  crmNewDeal: "New deal",
  crmEditDeal: "Edit deal",
  crmEdit: "Edit",
  crmCreate: "Create",
  crmSave: "Save",
  crmCancel: "Cancel",
  crmClose: "Close",
  crmDealSubtitle:
    "What the opportunity is, who it is with, and what it is worth.",
  crmFieldTitle: "Deal",
  crmFieldCompany: "Company",
  crmCompanyHint: "The company as your whole team should see it.",
  crmFieldContactName: "Contact",
  crmFieldContactEmail: "Contact email",
  crmContactEmailHint:
    "Used to suggest the conversations this deal belongs to.",
  crmFieldValue: "Value",
  crmValueHint: "What the deal is worth, before VAT.",
  crmFieldCurrency: "Currency",
  crmCurrencyHint: "Three letters, e.g. EUR.",
  crmFieldExpectedClose: "Expected close",
  crmFieldSource: "Source",
  crmSourceHint:
    "Where the opportunity came from — a referral, a campaign, a call.",
  crmNotAnAmount: "That is not an amount.",
  crmDeleteDeal: "Delete",
  crmDeleteDealConfirm:
    "This removes the deal and everything logged on it. Tasks raised from it stay in their owners' lists. It cannot be undone.",

  // The list
  crmDealsTable: "Deals",
  crmDealFilters: "Deal filters",
  crmSearchDeals: "Search deals",
  crmFilterStage: "Filter by stage",
  crmFilterAnyStage: "Any stage",
  crmFilterState: "Filter by state",
  crmFilterAnyState: "Any state",
  crmFilterMine: "Only mine",
  crmColDeal: "Deal",
  crmColCompany: "Company",
  crmColStage: "Stage",
  crmColValue: "Value",
  crmColExpectedClose: "Expected close",
  crmColState: "State",
  crmStateOpen: "Open",
  crmStateWon: "Won",
  crmStateLost: "Lost",
  crmExpectedClose: (day: string) => `Expected ${day}`,
  crmLostBecause: (reason: string) => `Lost: ${reason}`,

  // Losing a deal asks why, because a reason that is optional is a reason
  // nobody enters — and win/loss reporting is the feature.
  crmLostTitle: "Why was it lost?",
  crmLostMessage: (stage: string) =>
    `Moving this deal to “${stage}” closes it as lost. Say why, so the reason shows in your win/loss report.`,
  crmLostPlaceholder: "Price, timing, went to a competitor…",
  crmLostConfirm: "Mark as lost",
  crmLostReasonLabel: "Reason",
  crmLostReasonPrice: "Price",
  crmLostReasonTiming: "Timing",
  crmLostReasonCompetitor: "Chose a competitor",
  crmLostReasonBudget: "No budget",
  crmLostReasonNoDecision: "No decision",
  crmLostReasonNotAFit: "Not a fit",

  // Winning a deal: the handoff to billing. Both raise a DRAFT — nothing is
  // issued, nothing is sent, and no invoice number is used up.
  crmRaiseQuote: "Quote",
  crmRaiseInvoice: "Invoice",
  // Annotated `: string` — the catalog is `as const`, so an un-annotated
  // return of two literals types the key as *those two English words*, which
  // no translation could ever satisfy (B2.14).
  crmDocumentDraft: (kind: string): string =>
    kind === "invoice" ? "draft invoice" : "draft quote",
  crmRaiseTitle: (document: string) => `Raise a ${document}`,
  crmRaiseSubtitle:
    "It lands in Billing as a draft for you to check and complete. Nothing is issued and nothing is sent.",
  crmRaiseFrom: (deal: string, value: string) =>
    `From “${deal}”, worth ${value}.`,
  crmRaiseConfirm: "Raise it",
  crmRaiseFailed: "The document could not be raised.",
  crmFieldVatRate: "VAT rate",
  crmVatRateHint: "The rate this line is billed at, as a percentage — e.g. 21.",
  crmFieldCountry: "Customer country",
  crmCountryHint:
    "Two letters. This deal is still a lead, so a customer is created from it — and the country decides VAT treatment.",
  crmRaisedTitle: (document: string) => `Your ${document} is ready`,
  crmRaisedSubtitle:
    "Open it in Billing to check the lines, the address and the VAT.",
  crmRaisedWorth: (gross: string) => `${gross} including VAT.`,
  crmOpenInBilling: "Open in Billing",

  // The report: value by stage, and what was won and lost in a period. Every
  // figure is the server's, and currencies are never added together.
  crmReport: "Report",
  crmReportPeriod: "Report period",
  crmReportFrom: "From",
  crmReportTo: "To",
  crmReportShow: "Show",
  crmReportThisQuarter: "This quarter",
  crmReportLastQuarter: "Last quarter",
  crmReportDownloadCsv: "Download CSV",
  crmReportDownloadFailed: "The report could not be downloaded.",
  crmReportBasis: (from: string, to: string) =>
    `Won and lost between ${from} and ${to}.`,
  crmReportOpenAsOf: (at: string) =>
    `The open pipeline is as it stands at ${at}.`,
  crmReportOpenCaption: (currency: string) =>
    `Open pipeline by stage (${currency})`,
  crmReportClosedCaption: (currency: string) =>
    `Closed in the period (${currency})`,
  crmReportColDeals: "Deals",
  crmReportOpenTotal: "Open total",
  crmReportWinRate: (rate: string, won: number, closed: number) =>
    `Win rate ${rate} — ${won} of ${closed} closed deals.`,
  crmReportNoWinRate:
    "No deal closed in this period, so there is no win rate to show.",
  crmReportEmptyTitle: "Nothing to report yet",
  crmReportEmptyBody:
    "This board holds no deals. Raise one and it will appear here, by stage and by currency.",

  // The log
  crmActivityTitle: "Log",
  crmActivityKind: "Kind of entry",
  crmActivityPlaceholder: "What was said or agreed…",
  crmActivityAdd: "Log it",
  crmActivityDelete: "Delete entry",
  crmActivityEmpty: "Nothing logged yet.",
  crmKindNote: "Note",
  crmKindCall: "Call",
  crmKindMeeting: "Meeting",

  // Next steps — real tasks, in the list their owner already opens.
  crmNextStepsTitle: "Next steps",
  crmNextStepPlaceholder: "What happens next…",
  crmNextStepDue: "Due",
  crmNextStepAdd: "Add",
  crmNextStepsEmpty: "No next step agreed yet.",
  crmOpenInTasks: "Open in Tasks",

  // Linked conversations. Mail stays in mail: the link is a pointer, and only
  // a colleague who already holds the conversation can open it.
  crmThreadsTitle: "Conversations",
  crmThreadsEmpty: "No conversation linked yet.",
  crmThreadSuggest: "Suggest conversations",
  crmThreadLink: "Link",
  crmThreadUnlink: "Unlink",
  crmThreadOpenInMail: "Open in Mail",
  crmThreadNotYours:
    "This conversation is not in your mailbox — ask the colleague who linked it.",
  crmThreadLinkedBy: (who: string, when: string) =>
    `Linked by ${who} · ${when}`,
  crmSuggestionsEmpty:
    "Nothing in your recent mail matches this deal's addresses.",
  crmSuggestionAddress: (address: string) => `Matches ${address}`,
  crmSuggestionDomain: (address: string) => `Same company as ${address}`,

  // Sites (alo Sites, ADR 0036, wave S1). The rail says "Websites" — the
  // module is where a tenant's public sites are made, in the word a stranger
  // uses for them.
  moduleSites: "Websites",
  sitesLoadFailed: "Your websites could not be loaded.",
  sitesSiteLoadFailed: "This website could not be loaded.",
  sitesSaveFailed: "The change could not be saved.",
  sitesCheckFailed: "The address could not be checked.",
  sitesNewSite: "New website",
  sitesNoSitesTitle: "No websites yet",
  sitesNoSitesBody:
    "Build a site for your business and publish it under its own address.",
  sitesColName: "Name",
  sitesColAddress: "Address",
  sitesColStatus: "Status",
  sitesStatusDraft: "Draft",
  sitesStatusLive: "Live",
  sitesNewSiteTitle: "New website",
  sitesNewSiteSubtitle:
    "Start from a description, or choose one of the ready-made templates.",
  sitesStartingPoint: "How to start",
  sitesGenerateChoice: "Generate from a description",
  sitesGenerateChoiceDescription:
    "Tell alo about your business and get an editable first draft.",
  sitesTemplateChoice: "Start with a template",
  sitesTemplateChoiceDescription:
    "Choose a ready-made layout and customize it yourself.",
  sitesBusinessDescription: "Describe your business",
  sitesBusinessDescriptionHint:
    "Include what you offer, who it is for, and the tone you want. You can edit everything before publishing.",
  sitesBusinessDescriptionPlaceholder:
    "A neighborhood bakery making sourdough and celebration cakes for local families…",
  sitesGenerateSite: "Generate website",
  sitesGenerating: "Preparing your draft…",
  sitesCreatingSite: "Creating website…",
  sitesGenerationFailed:
    "Your draft could not be prepared. Check the server message and try again.",
  sitesGenerationEmpty:
    "The generated draft did not contain a page. Try a fuller description.",
  sitesGenerationUnavailable:
    "Generation is not configured for this workspace. Start with a blank site or choose a template below.",
  sitesChooseTemplate: "Choose a starting point",
  sitesBlankTemplate: "Blank site",
  sitesBlankTemplateSummary:
    "An empty Home page. You choose every section yourself.",
  sitesTemplatePageCount: (count: number) =>
    count === 1 ? "1 page" : `${count} pages`,
  sitesTemplatesLoading: "Loading the templates…",
  sitesTemplatesLoadFailed:
    "The templates could not be loaded. You can still start from a blank site.",
  sitesTemplatePreviewTitle: (name: string) => `Preview of ${name}`,
  sitesTemplatePreviewPages: "Pages in this template",
  sitesTemplatePreviewLoading: "Loading the preview…",
  sitesTemplatePreviewFailed:
    "This preview could not be loaded. You can still create the website from this template.",
  sitesTemplatePreviewNote:
    "A picture of the page. Switch pages above; every word and section is yours to edit afterwards.",
  sitesBlankPreviewNote:
    "You start with an empty Home page and add the sections you want.",
  sitesHomePageTitle: "Home",
  sitesAiEditTitle: "Describe a page change",
  sitesAiEditBody:
    "alo prepares a reviewable change list. Nothing changes until you approve it.",
  sitesAiInstruction: "Page change",
  sitesAiInstructionPlaceholder:
    "Make the welcome warmer and move testimonials above pricing…",
  sitesAiPropose: "Prepare changes",
  sitesAiPreparing: "Preparing changes…",
  sitesAiProposalTitle: "Proposed changes",
  sitesAiProposalCount: (count: number) =>
    count === 1 ? "1 proposed change" : `${count} proposed changes`,
  sitesAiPreviewHint:
    "Compare the page before and after, then choose what happens.",
  sitesAiPreviewCompare: "Compare proposed page changes",
  sitesInlineTextHint:
    "Click any text in the preview to edit it there. Enter saves it, Escape puts it back.",
  sitesInlineTextSaved: "Text updated.",
  sitesInlineTextUndone: "Text change undone.",
  sitesInlineTextRedone: "Text change redone.",
  sitesInlineTextStale:
    "That text belongs to a section that has since moved or changed. The preview has been refreshed — try the edit again.",
  sitesUndoEdit: "Undo last change",
  sitesRedoEdit: "Redo last change",
  sitesSectionDragHint:
    "Drag a section to move it — the page reorders as you drag. With the keyboard, focus a section and hold Alt with the up or down arrow.",
  sitesSectionResizeHint:
    "Some sections can change shape. Pick a size under the section in the list, or focus it in the preview and hold Alt with the left or right arrow.",
  sitesLayoutOf: (control: string) => `Choose ${control.toLowerCase()}`,
  sitesSectionResized: (section: string, choice: string) =>
    `${section} set to ${choice.toLowerCase()}.`,
  sitesLayoutSplit: "Split",
  sitesLayoutColumns: "Columns",
  sitesLayoutShape: "Shape",
  sitesLayoutSplitWideImage: "Wider image",
  sitesLayoutSplitHalf: "Equal halves",
  sitesLayoutSplitWideText: "Wider text",
  sitesLayoutColumnsTwo: "Two",
  sitesLayoutColumnsThree: "Three",
  sitesLayoutColumnsFour: "Four",
  sitesLayoutShapeNatural: "As uploaded",
  sitesLayoutShapeWide: "Wide",
  sitesLayoutShapeSquare: "Square",
  sitesLayoutShapeTall: "Tall",
  sitesSectionOnPage: (section: string, position: number, total: number) =>
    `${section}, section ${position} of ${total}. Drag it to move it, or hold Alt and press the up or down arrow.`,
  sitesAiPreviewBefore: "Before",
  sitesAiPreviewAfter: "After",
  sitesAiApprove: "Approve changes",
  sitesAiApplying: "Applying changes…",
  sitesAiDiscard: "Discard",
  sitesAiEditFailed:
    "The change list could not be prepared. Try again or edit the sections directly.",
  sitesAiApplyFailed:
    "These changes could not be applied. Review the server message and try again.",
  sitesAiAddChange: (section: string, position: number) =>
    `Add ${section} at position ${position}`,
  sitesAiRemoveChange: (section: string) => `Remove ${section}`,
  sitesAiMoveChange: (section: string, position: number) =>
    `Move ${section} to position ${position}`,
  sitesAiSettingChange: (section: string) => `Update a setting in ${section}`,
  sitesAiCopyChange: (section: string) => `Rewrite text in ${section}`,
  sitesAiImproveCopy: "Improve this copy",
  sitesAiCopyActions: "Copy improvements",
  sitesAiRewrite: "Rewrite",
  sitesAiShorter: "Make shorter",
  sitesAiLonger: "Add detail",
  sitesAiTone: "Desired tone",
  sitesAiTonePlaceholder: "Warm and direct",
  sitesAiUseTone: "Change tone",
  sitesAiCopyBefore: "Current copy",
  sitesAiCopyAfter: "Proposed copy",
  sitesAiCopyFailed:
    "This copy change could not be prepared. Try again or keep editing it directly.",
  sitesFieldName: "Site name",
  sitesFieldSubdomain: "Address",
  sitesSubdomainHint:
    "Lowercase letters, digits and hyphens, 3–40 characters — this becomes the site's web address.",
  sitesSubdomainChecking: "Checking availability…",
  sitesSubdomainAvailable: (subdomain: string) => `“${subdomain}” is free.`,
  sitesSubdomainTaken: (subdomain: string) =>
    `“${subdomain}” is already taken.`,
  sitesAddressAvailable: "Available",
  sitesAddressTaken: "Already taken",
  sitesAddressNotChecked: "Enter a valid address to check availability",
  sitesNameRequired: "Name your website to continue.",
  sitesAddressRequired: "Enter a site address to continue.",
  sitesCreateSite: "Create website",
  sitesCancel: "Cancel",
  sitesBack: "All websites",
  sitesCollaborators: "Collaborators",
  sitesCollaboratorsHint:
    "Invite people to edit and publish this website. They cannot open your mail, files, or other websites.",
  sitesCollaboratorEmail: "Email address",
  sitesCollaboratorEmailPlaceholder: "collaborator@example.com",
  sitesInviteCollaborator: "Invite editor",
  sitesCollaboratorsLoading: "Loading collaborators…",
  sitesCollaboratorsLoadFailed:
    "This website's collaborators could not be loaded.",
  sitesCollaboratorInviteFailed: "The collaborator could not be invited.",
  sitesCollaboratorRevokeFailed:
    "That collaborator's access could not be removed.",
  sitesCollaboratorCopyFailed:
    "The setup link could not be copied. Create a new link and try again.",
  sitesCollaboratorLinkReady: (email: string) =>
    `A private setup link is ready for ${email}. Copy it and share it securely.`,
  sitesCollaboratorAdded: (email: string) =>
    `${email} can now edit this website.`,
  sitesCollaboratorLinkCopied: "Setup link copied.",
  sitesCollaboratorRevoked: (email: string) => `${email}'s access was removed.`,
  sitesUndoCollaboratorRevoke: "Undo",
  sitesNoCollaborators:
    "Only you can edit this website. Enter an email above to invite its first collaborator.",
  sitesCollaboratorPending: "Invitation pending",
  sitesCollaboratorActive: "Can edit and publish",
  sitesRefreshCollaboratorLink: "New setup link",
  sitesCopyCollaboratorLink: "Copy setup link",
  sitesRevokeCollaborator: "Remove access",
  sitesInvitationHeading: "Join this website",
  sitesInvitationSubtitle: (site: string) =>
    `You have been invited to edit and publish ${site}.`,
  sitesInvitationLoading: "Checking your invitation…",
  sitesInvitationLoadFailed:
    "This invitation has expired or has already been used. Ask the website owner for a new link.",
  sitesInvitationPassword: "Create a password",
  sitesInvitationPasswordHint: "Use at least 8 characters.",
  sitesInvitationConfirmPassword: "Confirm password",
  sitesInvitationPasswordMismatch: "The passwords do not match.",
  sitesInvitationAccept: "Join website",
  sitesInvitationAccepting: "Joining…",
  sitesInvitationAcceptFailed: "Your invitation could not be accepted.",
  sitesInvitationDone: "You are ready to edit",
  sitesInvitationDoneBody: (email: string) =>
    `Sign in as ${email}. You will see only the websites shared with you.`,
  sitesInvitationSignIn: "Sign in to alo",
  sitesPages: "Pages",
  sitesPageCount: (count: number) =>
    `${count} ${count === 1 ? "page" : "pages"}`,
  sitesSiteTools: "Site tools",
  sitesSiteToolsHint: "Content, commerce, insights, and settings",
  sitesManageWebsite: "Manage your website",
  sitesManageWebsiteHint:
    "Publishing, collaborators, languages, and connected tools",
  sitesNewPage: "New page",
  sitesNoPagesTitle: "No pages yet",
  sitesNoPagesBody:
    "Every site starts with a home page. Add one to start building.",
  sitesColPage: "Page",
  sitesColPath: "Path",
  // Sites — blog authoring desk. alo Docs remains the source of every post.
  sitesPosts: "Blog posts",
  sitesBackToWebsite: "Website",
  sitesPostsLoadFailed: "Your blog posts could not be loaded.",
  sitesLoadingPosts: "Loading blog posts",
  sitesWriteInDocs: "Write in alo Docs",
  sitesOpeningDocs: "Opening alo Docs…",
  sitesUntitledArticle: "Untitled article",
  sitesPostCreateFailed: "The article could not be created. Try again.",
  sitesNoPostsTitle: "No articles yet",
  sitesNoPostsBody:
    "Start an article in alo Docs. It stays private until you choose to publish it.",
  sitesColArticle: "Article",
  sitesColUpdated: "Updated",
  sitesColActions: "Actions",
  sitesEditInDocs: "Edit in alo Docs",
  sitesPostStatusDraft: "Draft",
  sitesPostStatusPublished: "Published",
  sitesPublishArticle: "Publish",
  sitesPublishArticleTitle: "Publish article",
  sitesPublishArticleSubtitle:
    "Choose how the article will appear on your public website.",
  sitesEditArticleTitle: "Article details",
  sitesEditArticleSubtitle: "Update what readers see on your website.",
  sitesEditArticleDetails: "Edit details",
  sitesSaveArticle: "Save changes",
  sitesPostSaveFailed: "The article details could not be saved. Try again.",
  sitesPostUnpublishFailed:
    "The article could not be taken offline. Try again.",
  sitesUnpublishArticle: "Take offline",
  sitesUnpublishingArticle: "Taking offline…",
  sitesFieldPostTitle: "Article title",
  sitesFieldPostSlug: "Web address",
  sitesPostSlugHint: "Lowercase letters, digits and hyphens.",
  sitesPostSlugPlaceholder: "my-article",
  sitesFieldPostExcerpt: "Summary",
  sitesPostExcerptHint:
    "A short introduction shown on the blog page and in RSS.",
  sitesFieldPostCover: "Cover image",
  sitesPostCoverHint: "Shown on the blog page and above the article.",
  sitesPostNoCover: "No cover",
  sitesPostCoverAdded: "Cover added",
  sitesAddPostCover: "Add image",
  sitesReplacePostCover: "Replace image",
  sitesRemovePostCover: "Remove",
  sitesUploadingPostCover: "Uploading…",
  sitesPostCoverUploadFailed:
    "The cover image could not be uploaded. Try again.",
  sitesHomeBadge: "Home",
  sitesNewPageTitle: "New page",
  sitesNewPageSubtitle: "A page holds the sections you stack on it.",
  sitesFieldPageTitle: "Title",
  sitesFieldSlug: "Path",
  sitesLanguagesLabel: "Website languages",
  sitesEditingLanguage: "Editing language",
  sitesLanguages: "Languages",
  sitesLanguagesHint:
    "Add the languages visitors can choose and see exactly which pages still need translation.",
  sitesDefaultLanguage: "Default language",
  sitesAddLanguage: "Add a language",
  sitesLanguagePlaceholder: "Language code, for example fr",
  sitesAddLanguageAction: "Add language",
  sitesLanguageDefaultBadge: "Default",
  sitesRemoveLanguage: (language: string) => `Remove ${language}`,
  sitesLanguageSaveFailed:
    "The website languages could not be saved. Check the language code and try again.",
  sitesTranslationReady: "Ready",
  sitesTranslationProgress: (translated: number, total: number) =>
    `${translated} of ${total} pages translated`,
  sitesTranslationAllReady: "Every enabled language is ready to publish.",
  sitesTranslationPublishHint: (count: number) =>
    `${count} ${count === 1 ? "translation is" : "translations are"} still using fallback content.`,
  sitesContinueTranslating: "Continue translating",
  sitesTranslationSaveFailed:
    "This translation could not be saved. Fix the highlighted details and try again.",
  sitesTranslationMissingTitle: (locale: string) =>
    `${locale} needs a translation`,
  sitesTranslationMissingBody: (requested: string, source: string) =>
    `You are seeing the ${source} version for reference. Copy it into ${requested} to start translating without changing the source page.`,
  sitesCopyTranslation: (source: string, target: string) =>
    `Copy ${source} into ${target}`,
  sitesTranslationDetails: "Translated page details",
  sitesTranslationDetailsHint: (locale: string) =>
    `These title, path, and search details are shown only to ${locale} visitors.`,
  sitesSaveTranslation: "Save translation details",
  sitesTranslateWholeSite: "Translate whole site",
  sitesWholeTranslationPreparing:
    "Preparing a complete translation for review…",
  sitesWholeTranslationPrepareFailed:
    "The translation could not be prepared. Nothing changed; translate pages manually or try again.",
  sitesWholeTranslationApplyFailed:
    "The translation could not be applied. Nothing changed; prepare a fresh review and try again.",
  sitesWholeTranslationReview: (language: string) =>
    `Review the ${language} translation`,
  sitesWholeTranslationReviewHint:
    "Compare every page and post. Nothing is saved until you approve this review.",
  sitesWholeTranslationApprove: "Approve translation",
  sitesTranslationPageKind: "Page",
  sitesTranslationPostKind: "Post",
  sitesSlugHint:
    "Lowercase letters, digits and hyphens. The home page leaves this empty.",
  sitesFieldHome: "This is the home page",
  sitesCreatePage: "Create page",
  // Sites — the page editor (section stack + per-type prop forms).
  sitesPageLoadFailed: "This page could not be loaded.",
  sitesBackToSite: "All pages",
  sitesSections: "Sections",
  sitesAddSection: "Add section",
  sitesAddFirstSection: "Add your first section",
  sitesNoSectionsTitle: "Nothing on this page yet",
  sitesNoSectionsBody:
    "Stack sections — a hero, your features, a contact form — to build the page.",
  // The palette (ADR 0042 §4): blocks shown with the tenant's own content,
  // dragged onto the stack or placed with the keyboard.
  sitesPaletteTitle: "Add a section",
  sitesPaletteHint:
    "Drag a block onto the page, or choose where it goes and press one.",
  sitesPalettePosition: "Where it goes",
  sitesPaletteAtTop: "At the top",
  sitesPaletteAtEnd: "At the end",
  sitesPaletteAfter: (section: string) => `After the ${section}`,
  sitesPaletteAdd: (section: string, position: string) =>
    `Add ${section} — ${position.toLowerCase()}`,
  sitesPaletteDropHere: "Drop here to add at the end",
  sitesPaletteOwnContent: "Shown with your own content.",
  sitesPalettePreviewTitle: (section: string) => `${section} on your website`,
  sitesPaletteLoading: "Filling these with your own content…",
  sitesPaletteFailed:
    "Your own content could not be loaded, so these blocks open a form instead.",
  sitesPaletteOpensForm: "Opens a form",
  sitesPaletteDone: "Done adding",
  sitesPaletteNeedsWriting:
    "Nothing of yours belongs here yet — this one you write. Adding it opens a form.",
  sitesPaletteNeedsPicture:
    "Put a picture on this website and this block fills itself with it. Adding it now opens a form.",
  sitesPaletteNeedsCatalog:
    "Make a catalog first — this block shows what is in it. Adding it now opens a form.",
  sitesPaletteNeedsCollection:
    "Connect a collection first — this block shows its rows. Adding it now opens a form.",
  sitesPaletteNeedsBooking:
    "Add something people can book first — this block offers it. Adding it now opens a form.",
  sitesPaletteNeedsCode:
    "The code in this one is yours to write. Adding it opens a form.",
  sitesAddSectionTitle: (section: string) => `Add ${section}`,
  sitesEditSectionTitle: (section: string) => `Edit ${section}`,
  sitesSaveSection: "Save section",
  // Named, because a stack of five sections is otherwise twenty buttons
  // called "Move up", "Edit section", "Delete section" with nothing in the
  // name to say which one they act on (S2.16b2).
  sitesMoveUp: (section: string) => `Move ${section} up`,
  sitesMoveDown: (section: string) => `Move ${section} down`,
  sitesEditSection: (section: string) => `Edit ${section}`,
  sitesDeleteSection: (section: string) => `Delete ${section}`,
  sitesSectionMoved: (section: string, position: number, total: number) =>
    `${section} moved to position ${position} of ${total}.`,
  sitesSectionAdded: (section: string, position: number, total: number) =>
    `${section} added as section ${position} of ${total}.`,
  sitesConfirmDelete: "Really delete?",
  sitesPreview: "Preview",
  sitesPreviewTitle: "Draft preview",
  sitesPreviewDesktop: "Desktop width",
  sitesPreviewMobile: "Phone width",
  sitesPreviewFailed: "The preview could not be loaded.",
  sitesSeoAction: "Search & sharing",
  sitesSeoTitle: "Search & sharing",
  sitesSeoSubtitle:
    "Choose how this page appears in search results and shared links.",
  sitesSeoPreview: "Search result preview",
  sitesSeoFieldTitle: "Search title",
  sitesSeoTitleHint: "Leave blank to use the page title and website name.",
  sitesSeoFieldDescription: "Description",
  sitesSeoDescriptionHint:
    "A short, useful summary for search results and shared links.",
  sitesSeoDescriptionDefault:
    "Add a description so people know what this page is about.",
  sitesSeoImageHint:
    "Shared links use the page's first hero image. If there isn't one, your site logo is used.",
  sitesSeoSave: "Save search details",
  sitesSeoSaveFailed: "The search details could not be saved. Try again.",
  sitesSectionNav: "Navigation bar",
  sitesSectionNavDesc: "Links across the top of the page.",
  sitesSectionHero: "Hero",
  sitesSectionHeroDesc: "The big opening headline.",
  sitesSectionFeatures: "Features",
  sitesSectionFeaturesDesc: "A grid of what you offer.",
  sitesSectionTextImage: "Text & image",
  sitesSectionTextImageDesc: "A paragraph beside a picture.",
  sitesSectionGallery: "Gallery",
  sitesSectionGalleryDesc: "A wall of pictures.",
  sitesSectionTestimonials: "Testimonials",
  sitesSectionTestimonialsDesc: "Words from happy customers.",
  sitesSectionPricing: "Pricing",
  sitesSectionPricingDesc: "Your plans and their prices.",
  sitesSectionTeam: "Team",
  sitesSectionTeamDesc: "The people behind the business.",
  sitesSectionFaq: "FAQ",
  sitesSectionFaqDesc: "Questions people ask, answered.",
  sitesSectionCta: "Call to action",
  sitesSectionCtaDesc: "A banner that asks for the click.",
  sitesSectionContactForm: "Contact form",
  sitesSectionContactFormDesc: "Let visitors write to you.",
  sitesSectionFooter: "Footer",
  sitesSectionFooterDesc: "The line at the bottom of the page.",
  sitesCountLinks: (count: number) =>
    count === 1 ? "1 link" : `${count} links`,
  sitesCountImages: (count: number) =>
    count === 1 ? "1 image" : `${count} images`,
  sitesCountEntries: (count: number) =>
    count === 1 ? "1 entry" : `${count} entries`,
  sitesItemN: (position: number) => `Entry ${position}`,
  sitesRemoveItem: "Remove entry",
  sitesAddLink: "Add link",
  sitesAddEntry: "Add entry",
  sitesAddImage: "Add image",
  sitesAddTier: "Add plan",
  sitesAddMember: "Add person",
  sitesAddQuestion: "Add question",
  sitesFieldHeading: "Heading",
  sitesFieldSubheading: "Subheading",
  sitesFieldIntro: "Intro",
  sitesFieldBody: "Text",
  sitesFieldItemTitle: "Title",
  sitesFieldLinkLabel: "Link text",
  sitesFieldLinkHref: "Link target",
  sitesFieldButton: "Button",
  sitesFieldPrimaryButton: "Primary button",
  sitesFieldSecondaryButton: "Secondary button",
  sitesFieldImage: "Image",
  sitesFieldPhoto: "Photo",
  sitesFieldImageId: "Image ID",
  sitesImageIdHint:
    "Upload a picture, or paste an image ID from an earlier upload.",
  sitesFieldImageAlt: "Image description",
  sitesImageAltHint:
    "Read aloud by screen readers. Say what the picture shows; if it shows nothing that matters, mark it decorative below.",
  sitesImageAltMissing:
    "This picture has no description yet — say what it shows, or mark it decorative.",
  sitesImageDecorative: "Decorative — screen readers skip it",
  sitesImageDecorativeHint:
    "Only for pictures that carry no information of their own, such as a background pattern.",
  // Sites — framing a picture (crop + focal point).
  sitesImageFrameHint:
    "Drag on the picture to choose what stays visible. With the keyboard: arrow keys move the frame, shift with the arrow keys resizes it.",
  sitesImageFocalHint:
    "Drag the round marker onto whatever must stay in view when a layout has to crop the picture further.",
  sitesImageFrameAt: (
    width: number,
    height: number,
    left: number,
    top: number,
  ) =>
    `Visible area: ${width}% by ${height}% of the picture, ${left}% from the left and ${top}% from the top`,
  sitesImageFocalAt: (x: number, y: number) =>
    `Focal point ${x}% across and ${y}% down`,
  sitesImageFrameWidth: "Width",
  sitesImageFrameHeight: "Height",
  sitesImageFrameLeft: "Left",
  sitesImageFrameTop: "Top",
  sitesImageWholePicture: "Use the whole picture",
  sitesImageWholePictureState: "The whole picture is shown",
  sitesImageCentreFocal: "Centre the focal point",
  sitesImageNoPreview:
    "This picture cannot be shown here. The numbers below still frame it, and its description is unaffected.",
  // Sites — the AI draft of an image description.
  sitesAiAltWrite: "Suggest a description",
  sitesAiAltImprove: "Improve this description",
  sitesAiAltProposed: "Suggested description",
  sitesAiAltUnseen:
    "Drafted from the words in this section — alo has not seen the picture. Check it against the image before you approve it.",
  sitesAiAltFailed: "The description could not be drafted.",
  sitesFieldImageSide: "Picture side",
  sitesSideLeft: "Left",
  sitesSideRight: "Right",
  sitesFieldQuote: "Quote",
  sitesFieldAuthor: "Author",
  sitesFieldRole: "Role",
  sitesFieldTierName: "Plan name",
  sitesFieldPrice: "Price",
  sitesFieldPeriod: "Billing period",
  sitesFieldTierDescription: "Description",
  sitesFieldTierFeatures: "What's included",
  sitesTierFeaturesHint: "One line per bullet.",
  sitesFieldHighlighted: "Highlight this plan",
  sitesFieldMemberName: "Name",
  sitesFieldBio: "Bio",
  sitesFieldQuestion: "Question",
  sitesFieldAnswer: "Answer",
  sitesFieldSuccessMessage: "Message after sending",
  sitesFieldFooterText: "Footer text",
  sitesContactFormHint:
    "The form already shows on the page; sending starts working when forms arrive.",
  // Sites — theme (preset picker + logo/favicon upload).
  sitesTheme: "Theme",
  sitesThemeTitle: "Site theme",
  sitesThemeSubtitle: "Pick a look; add your logo and favicon.",
  sitesThemeApply: "Apply theme",
  sitesThemeLoadFailed: "The theme options could not be loaded.",
  sitesThemePresets: "Colors & type",
  sitesThemeLogo: "Logo",
  sitesThemeLogoHint: "Shown in the navigation bar instead of the site name.",
  sitesThemeFavicon: "Favicon",
  sitesThemeFaviconHint: "The little icon browsers show on the tab.",
  sitesThemeUpload: "Upload image",
  sitesThemeReplace: "Replace image",
  sitesThemeRemove: "Remove image",
  sitesThemeSet: "Image uploaded",
  sitesThemeNotSet: "None yet",
  sitesUploadFailed: "The image could not be uploaded.",
  sitesUploadImage: "Upload picture",
  sitesPublish: "Publish",
  sitesPublishChanges: "Publish changes",
  sitesUnpublish: "Take offline",
  sitesConfirmUnpublish: "Really take offline?",
  sitesLiveAtLabel: "Your site is live at",
  sitesGoesLiveAt: (address: string) =>
    `Publishing puts this site live at ${address}.`,
  sitesAddressPreview: (address: string) =>
    `Your site will live at ${address}.`,
  sitesPublishFailed: "The site could not be published.",
  sitesUnpublishFailed: "The site could not be taken offline.",
  // Sites — the contact-form inbox.
  sitesSubmissions: "Submissions",
  sitesSubmissionsLoadFailed: "Your form submissions could not be loaded.",
  sitesSubmissionSaveFailed: "That submission could not be updated.",
  sitesNoSubmissionsTitle: "No messages yet",
  sitesNoSubmissionsBody:
    "Add a contact form to a page. New visitor messages will appear here.",
  sitesOpenPages: "Open pages",
  sitesSubmissionList: "Visitor messages",
  sitesSubmissionDetail: "Selected visitor message",
  sitesHandled: "Handled",
  sitesNeedsReply: "Needs reply",
  sitesMarkHandled: "Mark handled",
  sitesReopenSubmission: "Reopen",
  sitesForm: "Form",
  sitesReceived: "Received",
  sitesExportSubmissions: "Export CSV",
  sitesExportingSubmissions: "Preparing export…",
  sitesSubmissionsExportFailed:
    "Your submissions could not be exported. Try again.",
  // Sites — the visitor assistant's admin screen (ADR 0040, S3.02d).
  sitesAssistant: "Assistant",
  sitesAssistantTitle: "Site assistant",
  sitesAssistantLoadFailed:
    "The assistant's settings could not be loaded. Try again.",
  sitesAssistantSwitchTitle: "The assistant and its budget",
  sitesAssistantSwitchHint:
    "A chat assistant on your published website that answers visitors' questions from your published pages — and always names the page an answer came from.",
  sitesAssistantEnable: "Answer visitors' questions on the published website",
  sitesAssistantBudgetLabel: "Monthly budget (€)",
  sitesAssistantBudgetHint: (defaultBudget: string) =>
    `Answers cost money. When a month's answers reach this budget, the assistant pauses and visitors are pointed to your contact form instead — you will be notified. If you leave it unset, the budget is ${defaultBudget}.`,
  sitesAssistantBudgetNotANumber:
    "Enter the monthly budget as a number of euros.",
  sitesAssistantSpent: (spent: string, budget: string) =>
    `${spent} of ${budget} spent this month.`,
  sitesAssistantCeilingHit:
    "This month's budget is used up, so the assistant is paused and visitors are offered your contact form. Raising the budget reopens it immediately.",
  sitesAssistantSave: "Save",
  sitesAssistantSaved: "Saved.",
  sitesAssistantSaveFailed:
    "The assistant's settings could not be saved. Try again.",
  sitesAssistantReadsTitle: "What the assistant reads",
  sitesAssistantReadsRule:
    "Whatever the assistant can read, anyone on the internet can read — it answers strangers with it.",
  sitesAssistantReadsPublishedSite: "Your published website — every live page",
  sitesAssistantReadsPublishedPosts: "Your published blog posts",
  sitesAssistantAlwaysRead: "always read",
  sitesAssistantNoKnowledge:
    "No documents published to the assistant yet. It answers from your published website alone.",
  sitesAssistantAddedOn: (date: string) => `published ${date}`,
  sitesAssistantTrashed: "in the Drive trash — no longer read",
  sitesAssistantWithdraw: (title: string) => `Withdraw ${title}`,
  sitesAssistantWithdrawFailed:
    "The document could not be withdrawn from the assistant. Try again.",
  sitesAssistantInternetWarning:
    "Anyone on the internet will be able to read this.",
  sitesAssistantPublishDocument: "Publish a document to the assistant…",
  sitesAssistantPublishFailed:
    "The document could not be published to the assistant. Try again.",
  sitesAssistantPickerTitle: "Publish a document to the assistant",
  sitesAssistantPickerSubtitle:
    "Pick one readable document — the assistant will answer visitors from it.",
  sitesAssistantPickerConfirm: "Publish to the assistant",
  sitesAssistantPickerBack: "Back to the parent folder",
  sitesAssistantPickerSearch: "Search this folder",
  sitesAssistantPickerEmpty: "Nothing in this folder.",
  // Sites — the assistant's action transcript (ADR 0040, S3.03e).
  sitesAssistantDidTitle: "What the assistant did",
  sitesAssistantDidHint:
    "Each action the assistant took on your behalf, with the fact it used and the page that fact came from. What visitors typed is never stored.",
  sitesAssistantDidEmpty:
    "Nothing yet. When the assistant answers a question, offers free times, books a meeting, or saves a lead, each action appears here.",
  sitesAssistantDidLoadFailed:
    "What the assistant did could not be loaded. Try again.",
  sitesAssistantDidAnswered: "Answered a question",
  sitesAssistantDidAnsweredUsing: (pages: string) =>
    `Answered a question using ${pages}`,
  sitesAssistantDidRefused:
    "Declined a question it could not answer from your published pages",
  sitesAssistantDidBookingOffered: (service: string) =>
    `Offered free times for “${service}”`,
  sitesAssistantDidBooked: (service: string, when: string) =>
    `Booked “${service}” for ${when} — the appointment is in your calendar`,
  sitesAssistantDidLeadOffered: "Offered the contact form in the conversation",
  sitesAssistantDidLeadSaved: "Saved a new lead to your CRM board",
  sitesAssistantDidLeadKnown:
    "Told a returning contact you already know them — no duplicate was made",
  sitesAssistantDidTicketsOffered: (event: string) =>
    `Offered tickets for “${event}” at the price list's own price`,
  // Sites — the assistant's appearance screen (ADR 0040 §5, S3.02g).
  sitesAssistantLookTitle: "How it looks and speaks",
  sitesAssistantLookHint:
    "The widget already wears your site's theme, logo, and language. What you choose here is its words and a few bounded choices — colour stays within your site's own palette.",
  sitesAssistantBotNameLabel: "Assistant name",
  sitesAssistantBotNameHint:
    "Often deliberately not the company name — “Ask Marie” outperforms “Chat with us”.",
  sitesAssistantAvatarLabel: "Avatar",
  sitesAssistantAvatarHint:
    "A small photo shown in the widget's header. A face works better than a logo.",
  sitesAssistantWelcomeLabel: "Welcome message",
  sitesAssistantWelcomeDefaultNote:
    "This is the written default, in your site's language — keep it or make it yours.",
  sitesAssistantQuestionsLegend: "Suggested questions",
  sitesAssistantQuestionsHint:
    "Up to three one-tap questions, offered until the visitor asks their own.",
  sitesAssistantQuestionLabel: (n: number) => `Suggested question ${n}`,
  sitesAssistantSuggestFromSite: "Suggest from your site",
  sitesAssistantSuggestedApplied:
    "Drafted from your site's own pages — edit them freely.",
  sitesAssistantSuggestedNone:
    "Nothing to draft from yet. An FAQ, pricing, booking, or contact section on your pages gives this something to work with.",
  sitesAssistantSuggestFailed:
    "Your pages could not be read for suggestions. Try again.",
  sitesAssistantSuggestedPricing: "What does it cost?",
  sitesAssistantSuggestedBooking: "Can I book an appointment?",
  sitesAssistantSuggestedCatalog: "What do you offer?",
  sitesAssistantSuggestedContact: "How can I reach you?",
  sitesAssistantAppearanceSave: "Save appearance",
  sitesAssistantToneLegend: "Tone",
  sitesAssistantToneFormal: "Formal",
  sitesAssistantToneNeutral: "Neutral",
  sitesAssistantToneWarm: "Warm",
  sitesAssistantToneNoteLabel: "Voice note",
  sitesAssistantToneNoteHint:
    "How your business speaks — plain words, no jargon, that sort of thing. Style only: it can never change what the assistant is allowed to say or promise.",
  sitesAssistantCornerLegend: "Launcher corner",
  sitesAssistantCornerRight: "Bottom right",
  sitesAssistantCornerLeft: "Bottom left",
  sitesAssistantIconLegend: "Launcher icon",
  sitesAssistantIconChat: "Speech bubble",
  sitesAssistantIconQuestion: "Question mark",
  sitesAssistantIconSparkle: "Sparkle",
  sitesAssistantAccentLegend: "Colour",
  sitesAssistantAccentHint:
    "A choice among your site's own palette roles — every option keeps readable contrast.",
  sitesAssistantAccentPrimary: "Brand colour",
  sitesAssistantAccentText: "Ink",
  sitesAssistantAccentSurface: "Quiet",
  sitesAssistantAutoOpenLabel: "Open by itself when the page loads",
  sitesAssistantAutoOpenHint:
    "Off by default — an uninvited popup is the thing everyone hates. When on, it opens without stealing the keyboard.",
  sitesAssistantOfflineLabel: "Offline message",
  sitesAssistantOfflineHint:
    "Shown when the assistant cannot answer — the monthly budget is spent, or no AI is configured.",
  sitesAssistantPreviewTitle: "Preview",
  sitesAssistantPreviewHint:
    "The real widget, wearing your site's theme, shown open. Visitors see it closed in its corner first.",
  sitesAssistantPreviewFrameTitle: "Assistant widget preview",
  sitesAssistantPreviewFailed: "The preview could not be rendered.",
  sitesAssistantA11yTitle: "Accessibility",
  sitesAssistantA11yContrast: (ratio: string) =>
    `Text on the chosen colour measures ${ratio}:1 — above the 4.5:1 WCAG AA bar.`,
  sitesAssistantA11yContrastGuarantee:
    "Every colour choice here is contrast-checked against your palette on the server — no option can store an unreadable combination.",
  sitesAssistantA11yKeyboard:
    "The widget is a labelled dialog: keyboard-operable throughout, Escape closes it, and answers are announced by screen readers as they arrive.",
  sitesAssistantA11yAvatar:
    "The avatar is decorative and hidden from screen readers — the assistant's name is what they announce.",
  // Sites — privacy-friendly traffic analytics.
  sitesAnalytics: "Analytics",
  sitesAnalyticsLoadFailed:
    "Your site analytics could not be loaded. Try again.",
  sitesAnalyticsLoading: "Loading site analytics",
  sitesAnalyticsPeriod: "Analytics period",
  sitesAnalyticsDays: (days: number) => `${days} days`,
  sitesAnalyticsSummary: "Traffic summary",
  sitesAnalyticsVisits: "Visits",
  sitesAnalyticsVisitors: "Daily visitors",
  sitesAnalyticsOverTime: "Visits over time",
  sitesAnalyticsChartLabel: "Daily site visits",
  sitesAnalyticsDayLabel: (date: string, visits: number) =>
    `${date}: ${visits} ${visits === 1 ? "visit" : "visits"}`,
  sitesAnalyticsTopPages: "Top pages",
  sitesAnalyticsTopReferrers: "Top referrers",
  sitesAnalyticsDirect: "Direct",
  sitesAnalyticsPrivacyTitle: "No cookies. No banner.",
  sitesAnalyticsPrivacyBody:
    "Traffic is counted anonymously by day. alo stores no visitor address, device profile, or browsing history.",
  sitesAnalyticsPrivacyBeacon:
    "Reading time and outbound clicks are reported by a small script on your pages. It carries no identity at all, so two reports from the same browser cannot be linked.",
  sitesAnalyticsEmptyTitle: "No visits yet",
  sitesAnalyticsEmptyBody:
    "Open or share your published site. Its first visits will appear here automatically.",
  sitesAnalyticsOpenSite: "Open live site",
  // Sites — the grouped detail panels (S2.08b). Each panel says how its
  // numbers get there, because an aggregate read as something it is not is
  // worse than no aggregate.
  sitesAnalyticsGroupArrival: "How people found you",
  sitesAnalyticsGroupPages: "What they looked at",
  sitesAnalyticsGroupReading: "How they read it",
  sitesAnalyticsShowAll: (count: number) => `Show all ${count}`,
  sitesAnalyticsShowTop: (count: number) => `Show top ${count}`,
  sitesAnalyticsReferrersNote:
    "The website a visitor followed a link from. Only the domain is kept, never the page.",
  sitesAnalyticsReferrersEmpty:
    "No referrers yet. They appear when another website links to yours.",
  sitesAnalyticsCampaigns: "Campaigns",
  sitesAnalyticsCampaignsNote:
    "Read from utm_campaign on the links you share, so you can tell a newsletter from a poster.",
  sitesAnalyticsCampaignsEmpty:
    "No campaigns yet. Add ?utm_campaign=spring-mailing to a link you share and its visits are counted here.",
  sitesAnalyticsNoCampaign: "No campaign",
  sitesAnalyticsCountries: "Countries",
  sitesAnalyticsCountriesNote:
    "Resolved by the network in front of your site, never from a stored visitor address.",
  sitesAnalyticsCountriesEmpty:
    "No countries reported. Your site is served without a network that names them, so this stays empty — every other number here is unaffected.",
  sitesAnalyticsNotReported: "Not reported",
  sitesAnalyticsTopPagesNote: "The pages that were opened most.",
  sitesAnalyticsPagesEmpty: "No pages counted in this period yet.",
  sitesAnalyticsEntryPages: "First pages",
  sitesAnalyticsEntryPagesNote:
    "The page a visitor's day on your site started on.",
  sitesAnalyticsExitPages: "Last pages",
  sitesAnalyticsExitPagesNote:
    "The last page seen that day. A last page is where someone finished reading, not necessarily where they gave up.",
  sitesAnalyticsReadTime: "Reading time",
  sitesAnalyticsReadTimeNote:
    "How long pages stayed on screen, for the whole site rather than per page. Only browsers that report it are counted, so these never add up to your visits.",
  sitesAnalyticsReadTimeEmpty:
    "No reading times yet. They arrive once visitors open your published pages in a browser that reports them.",
  sitesAnalyticsReadUnder10s: "Under 10 seconds",
  sitesAnalyticsRead10to30s: "10–30 seconds",
  sitesAnalyticsRead30to60s: "30–60 seconds",
  sitesAnalyticsRead1to3m: "1–3 minutes",
  sitesAnalyticsRead3to10m: "3–10 minutes",
  sitesAnalyticsReadOver10m: "Over 10 minutes",
  sitesAnalyticsOutbound: "Links away",
  sitesAnalyticsOutboundNote:
    "Domains visitors left for. Past 200 destinations in a day, the rest are counted together.",
  sitesAnalyticsOutboundEmpty:
    "No outbound clicks yet. They are counted when a visitor follows a link to another website.",
  sitesAnalyticsOutboundOther: "Other domains",
  sitesAnalyticsDevices: "Devices",
  sitesAnalyticsDevicesNote:
    "A coarse class from what the browser says about itself. Nothing more of it is stored.",
  sitesAnalyticsDevicesEmpty: "No devices counted in this period yet.",
  sitesAnalyticsDevicePhone: "Phone",
  sitesAnalyticsDeviceTablet: "Tablet",
  sitesAnalyticsDeviceDesktop: "Computer",
  sitesAnalyticsDeviceBot: "Bots and crawlers",
  sitesAnalyticsDeviceUnknown: "Unrecognised",
  // Sites — the attention map (S2.09b): the aggregate clicks and reading
  // depth collected in S2.09a. Every string here works to stop one
  // misreading — that a shape counted per area of the page is a count of
  // people, or that a map drawn from a handful of clicks means anything.
  sitesHeatmap: "Attention map",
  sitesBackToAnalytics: "Back to analytics",
  sitesHeatmapLoadFailed: "The attention map could not be loaded. Try again.",
  sitesHeatmapLoading: "Loading the attention map",
  sitesHeatmapPage: "Page",
  sitesHeatmapPageOption: (path: string, events: number) =>
    `${path} — ${events} counted`,
  sitesHeatmapScreens: "Screen size",
  sitesHeatmapScreenTab: (screen: string, events: string) =>
    `${screen} (${events})`,
  sitesHeatmapPrivacyTitle: "A shape, not a recording.",
  sitesHeatmapPrivacyBody:
    "Clicks and reading depth are counted per area of the page, by day. There is no cursor trail, no session replay, and nothing that can link two visits to the same person.",
  sitesHeatmapPrivacyShape:
    "Only browsers that report it are counted, and at most twenty clicks per page view. Read this as where attention went — never as how many people did something.",
  sitesHeatmapEmptyTitle: "Nothing to map yet",
  sitesHeatmapEmptyBody:
    "Clicks and reading depth appear here once visitors open your published pages. Nothing needs switching on.",
  sitesHeatmapClicks: "Where people clicked",
  sitesHeatmapClicksNote:
    "The whole page, top to bottom, not one screenful. A darker square is an area that was clicked more.",
  sitesHeatmapClicksLabel: (path: string, screen: string, clicks: number) =>
    `Map of where ${clicks} clicks landed on ${path}, on a ${screen}`,
  sitesHeatmapTop: "Top of the page",
  sitesHeatmapBottom: "Bottom of the page",
  sitesHeatmapLegendQuiet: "Quieter",
  sitesHeatmapLegendBusy: "Busier",
  sitesHeatmapLeft: "Left",
  sitesHeatmapCentre: "Centre",
  sitesHeatmapRight: "Right",
  sitesHeatmapSpot: (side: string, band: string) => `${side}, ${band}`,
  sitesHeatmapDepthBand: (from: number, to: number) => `${from}–${to}% down`,
  sitesHeatmapSpots: "Busiest areas",
  sitesHeatmapSpotsNote:
    "The same map in words, so it can be read without the colours.",
  sitesHeatmapClicksEmpty:
    "Nothing has been clicked on this page on this screen size.",
  sitesHeatmapSpotsEmpty: "Nothing to describe yet.",
  sitesHeatmapSpotsHeldBack:
    "Held back until enough clicks have been counted to describe.",
  sitesHeatmapDepth: "How far they read",
  sitesHeatmapDepthNote:
    "How many readers reached each tenth of the page. Only browsers that report it are counted, so this never adds up to your visits.",
  sitesHeatmapDepthEmpty: "No reading depth counted here on this screen size.",
  sitesHeatmapTooFewTitle: "Too little to draw a map",
  sitesHeatmapTooFewClicks: (collected: number, needed: number) =>
    `${collected} of ${needed} clicks counted on this screen size. A map drawn from a handful of clicks shows the handful, not your visitors — so it is kept back until there are enough.`,
  sitesHeatmapTooFewDepth: (collected: number, needed: number) =>
    `${collected} of ${needed} reading reports counted on this screen size. The curve appears once there are enough for it to mean anything.`,
  // Sites — what the website was worth (S2.10c): the arc from a page view to
  // an invoice, read over the CRM/Billing seam built in S2.10b. Every string
  // here carries one of the four facts that keep these numbers honest — where
  // each step's number comes from, that the per-form columns are not addends,
  // that currencies are never converted, and what "invoices" actually counts.
  sitesFunnel: "Results",
  sitesFunnelPeriod: "Period",
  sitesFunnelLoading: "Loading the results",
  sitesFunnelLoadFailed: "The results could not be loaded. Try again.",
  sitesFunnelDeniedTitle: "Not part of your access",
  sitesFunnelDeniedFallback:
    "This page reads alo CRM and alo Billing, which are not open for this account.",
  sitesFunnelDeniedWay:
    "Everything else about this website — its pages, its enquiries and its traffic — is still yours to work on.",
  sitesFunnelNoSourcesTitle: "No contact form yet",
  sitesFunnelNoSourcesBody:
    "Add a contact form to a page, and every enquiry it brings in can be followed from the first page view to the invoice.",
  sitesFunnelChain: "From visitor to invoice",
  sitesFunnelStageViews: "Saw the form",
  sitesFunnelStageStarts: "Started typing",
  sitesFunnelStageSubmits: "Enquiries",
  sitesFunnelStageLeads: "Handed to sales",
  sitesFunnelStageWon: "Won",
  sitesFunnelStageInvoices: "Invoices",
  sitesFunnelFromBrowser: "Reported by the browser",
  sitesFunnelFromRecord: "Counted when it was saved",
  sitesFunnelFloorNote:
    "The first two steps are reported by the visitor's browser, and a browser that reports nothing still saw the page. Everything from the enquiry onwards is counted when the record was written. Read these as a floor: a rate across that line is the smallest it could be, not a measurement.",
  sitesFunnelMoney: "The money behind it",
  sitesFunnelInvoiceRule:
    "Invoices raised for the customer an enquiry became, after it was handed over.",
  sitesFunnelMoneyEmpty:
    "No opportunity has been raised from this website yet.",
  sitesFunnelOpen: "Being worked",
  sitesFunnelWon: "Won",
  sitesFunnelInvoiced: "Invoiced",
  sitesFunnelHidden: "Not shown",
  sitesFunnelBillingOff:
    "Invoice figures are not shown because alo Billing is not open for this account. That is not the same as nothing having been invoiced.",
  sitesFunnelCurrencies:
    "Two currencies are two lines and no total: a forecast has no issue date to convert at.",
  sitesFunnelSources: "Per contact form",
  sitesFunnelColSource: "Contact form",
  sitesFunnelColDeals: "Opportunities",
  sitesFunnelDealsSummary: (open: number, won: number, lost: number) =>
    `${open} open · ${won} won · ${lost} lost`,
  sitesFunnelSumNote:
    "One invoice reachable from two forms counts once for the website and once under each form, so these columns are a reading per form and do not add up to the totals above.",
  sitesFunnelDeletedSource: "Deleted form",
  sitesFunnelChatSource: "Website assistant",
  // Sites — handing one enquiry to the sales board (S2.10c). The dialog asks
  // for the three things only a person can decide; the enquirer's name,
  // address and message travel with the handoff and are never retyped.
  sitesHandoffSection: "Sales",
  sitesHandoffInvite:
    "Turn this enquiry into an opportunity on your sales board. Nothing on this screen needs typing again.",
  sitesHandoffTitle: "Hand this enquiry to sales",
  sitesHandoffSubtitle:
    "Raises an opportunity on your sales board and links it to this enquiry.",
  sitesHandoffSubmit: "Hand to sales",
  sitesHandoffFrom: "From",
  sitesHandoffCarried:
    "The name, the address and the message travel with the handoff — you never retype them.",
  sitesHandoffTitleFor: (who: string) => `Website enquiry — ${who}`,
  sitesHandoffBoard: "Board",
  sitesHandoffColumn: "Column",
  sitesHandoffCardTitle: "Opportunity",
  sitesHandoffValue: "Expected value",
  sitesHandoffValueHint: "Optional — what you think it could be worth.",
  sitesHandoffCurrency: "Currency",
  sitesHandoffCurrencyHint: "Leave empty for your workspace currency.",
  sitesHandoffLoadingBoards: "Loading your sales boards…",
  sitesHandoffNoBoards:
    "There is no sales board to hand this to yet. Open alo CRM once and your first board is made for you.",
  sitesHandoffCrmDenied: "alo CRM is not open for this account.",
  sitesHandoffBoardsFailed: "Your sales boards could not be loaded. Try again.",
  sitesHandoffFailed: "This enquiry could not be handed over. Try again.",
  sitesInSales: "In sales",
  sitesLeadsLoadFailed: "The sales links for this inbox could not be loaded.",
  sitesLeadStanding: (state: string, value: string) => `${state} · ${value}`,
  sitesLeadOpen: "Being worked",
  sitesLeadWon: "Won",
  sitesLeadLost: "Lost",
  sitesUnlinkLead: "Unlink",
  sitesUnlinkLeadFailed:
    "The link could not be removed. The opportunity itself is untouched. Try again.",
  // Sites — every version this website has published (S2.04b). The list is
  // dates, never version ids: a person recognises "yesterday at 14:20", not
  // an opaque token.
  sitesHistory: "Version history",
  sitesHistorySubtitle:
    "Every version of this website you have published. Look at any of them, and put one back online in one click.",
  sitesHistoryLoadFailed: "The version history could not be loaded.",
  sitesHistoryVersions: "Published versions",
  sitesHistoryLiveNow: "Live now",
  sitesHistoryVersionOf: (date: string) => `Version of ${date}`,
  sitesHistoryPagesCount: (pages: number) =>
    `${pages} ${pages === 1 ? "page" : "pages"}`,
  sitesHistoryLanguages: (languages: string) => `Languages: ${languages}`,
  sitesHistoryRestoredCopy: (date: string) =>
    `A copy of the version of ${date}`,
  sitesHistoryRestore: "Put this version back online",
  sitesHistoryRestoring: "Putting it back online…",
  sitesHistoryRestoreFailed: "That version could not be put back online.",
  sitesHistoryRestored: (date: string) =>
    `The version of ${date} is back online.`,
  sitesHistoryUndo: "Undo",
  sitesHistoryUndone: (date: string) =>
    `Back to the version of ${date}. Nothing was lost — every version is still here.`,
  sitesHistoryPage: "Page",
  sitesHistoryPreviewLoadFailed: "That version could not be shown.",
  sitesHistoryPreviewLoading: "Loading this version",
  sitesHistoryPreviewTitle: "Published version preview",
  sitesHistoryDraftSafe:
    "Your work in progress is untouched: putting a version back online never changes what you are editing.",
  sitesHistoryIfRestored: "If you put this version back online",
  sitesHistoryIdentical: "This is exactly what is live now.",
  sitesHistoryThemeChange: "The look of the site would change.",
  sitesHistoryLanguagesBack: (languages: string) =>
    `These languages would come back: ${languages}`,
  sitesHistoryLanguagesGone: (languages: string) =>
    `These languages would go away: ${languages}`,
  sitesHistoryPageBack: (page: string) => `${page} would come back`,
  sitesHistoryPageGone: (page: string) => `${page} would go away`,
  sitesHistoryPageChanged: (page: string) => `${page} would change`,
  sitesHistoryUnchangedPages: (pages: number) =>
    `${pages} ${pages === 1 ? "page stays" : "pages stay"} the same`,
  sitesHistoryEmptyTitle: "Nothing published yet",
  sitesHistoryEmptyBody:
    "Publish this website once, and every version you publish stays here — to look back at, and to put back online.",

  // Sites — publishing at a chosen moment (S2.05b). Every moment shown here
  // is in the reader's own time, and the zone is named beside the picker:
  // someone scheduling a launch from another country must be able to see
  // which nine o'clock they picked.
  sitesScheduleTitle: "Publish at a chosen moment",
  sitesScheduleHint:
    "Pick a date and time, and this website goes live by itself. You do not have to be here when it does.",
  sitesScheduleLoading: "Checking what is scheduled",
  sitesScheduleLoadFailed: "The scheduled publishing could not be loaded.",
  sitesScheduleOpen: "Schedule publishing",
  sitesScheduleChange: "Change the moment",
  sitesScheduleWhen: "Date and time",
  sitesScheduleGoesLive: (moment: string) => `Goes live on ${moment}.`,
  sitesScheduleTimeZone: (zone: string) =>
    `That is your own time (${zone}) — not the server's.`,
  sitesScheduleSave: "Schedule publishing",
  sitesScheduleMove: "Move to this moment",
  sitesScheduleSaving: "Saving…",
  sitesScheduleMissingMoment: "Choose a date and time first.",
  sitesScheduleSaveFailed: "This website could not be scheduled.",
  sitesSchedulePending: (moment: string) =>
    `This website publishes itself on ${moment}. Everything you save until then goes live with it.`,
  sitesSchedulePublishingNow: "This website is being published right now.",
  sitesScheduleCancel: "Call it off",
  sitesScheduleCancelling: "Calling it off…",
  sitesScheduleCancelFailed:
    "The scheduled publishing could not be called off.",
  sitesScheduleCancelled: (moment: string) =>
    `Called off. This website will not publish on ${moment}, and nothing that is online has changed.`,
  sitesScheduleDone: (moment: string) =>
    `This website published itself on ${moment}.`,
  sitesScheduleFailed: (moment: string, reason: string) =>
    `This website could not publish on ${moment}: ${reason}`,

  // Sites — a page behind a password (S2.06b). The copy says who can READ the
  // page, never what the setting is called, and it tells the owner what the
  // visitor meets: an unlock screen that shows nothing of the page, not even
  // its title. An owner who expects to see the page's own name there would
  // otherwise think it broke.
  sitesPagePasswordTitle: "Who can open this page",
  sitesPagePasswordLoading: "Checking who can open this page",
  sitesPagePasswordLoadFailed:
    "Whether this page asks for a password could not be checked.",
  sitesPagePasswordUnknown:
    "Whether this page asks visitors for a password is not known right now.",
  sitesPagePasswordPublic: "Anyone on the internet can open this page.",
  sitesPagePasswordPublicHint:
    "Give it a password and only the people you hand it to can read it. The rest of this website stays public.",
  sitesPagePasswordProtected: (moment: string) =>
    `Only people with the password can open this page — set on ${moment}.`,
  sitesPagePasswordProtectedUndated:
    "Only people with the password can open this page.",
  sitesPagePasswordProtectedHint:
    "Everyone else meets an unlock screen carrying nothing of the page, not even its title. The password opens it for the rest of the day.",
  sitesPagePasswordEveryLanguage:
    "This holds for the page in every language it is published in.",
  sitesPagePasswordProtect: "Protect this page",
  sitesPagePasswordChange: "Change the password",
  sitesPagePasswordField: "Password",
  sitesPagePasswordFieldHint:
    "Nobody can read this back to you afterwards, us included — a forgotten password is replaced, not recovered.",
  sitesPagePasswordEffective:
    "It takes effect at once. You do not have to publish the website again.",
  sitesPagePasswordShow: "Show",
  sitesPagePasswordHide: "Hide",
  sitesPagePasswordSaving: "Saving…",
  sitesPagePasswordMissing: "Type a password first.",
  sitesPagePasswordSaveFailed: "This page could not be protected.",
  sitesPagePasswordSaved:
    "Saved. Visitors need this password from now on, and anyone who opened the page with the old one is asked again.",
  sitesPagePasswordRemove: "Remove the password",
  sitesPagePasswordRemoveConfirm: "Yes, make it public",
  sitesPagePasswordRemoveFailed: "The password could not be removed.",
  sitesPagePasswordRemoved:
    "The password is gone. Anyone on the internet can open this page again.",
  sitesPagePasswordPreviewNote:
    "Visitors are asked for the password first. This preview shows the page as someone who has it sees it.",
  sitesPagePasswordBadge: "Password",

  // Audit trail — a record's own history (B2.13). The labels are VERBS, not
  // sentences: the record kind is the page the reader is already on, so an
  // invoice's history says "Issued" rather than "Invoice issued". Keep them
  // that way in every language, and keep them past tense — each line is a
  // thing that happened.
  auditHistoryTitle: "History",
  auditHistoryEmpty: "Nothing has happened to this record yet.",
  auditLoadFailed: "The history could not be loaded.",
  auditActionCreate: "Created",
  auditActionUpdate: "Edited",
  auditActionDelete: "Deleted",
  auditActionArchive: "Archived",
  auditActionIssue: "Issued",
  auditActionVoid: "Voided",
  auditActionCreditNote: "Credit note raised",
  auditActionSend: "Email drafted",
  auditActionReminder: "Reminder drafted",
  auditActionPaymentCreate: "Payment recorded",
  auditActionPaymentDelete: "Payment removed",
  auditActionImport: "Imported",
  auditActionSepaXml: "Added to a payment file",
  auditActionApprove: "Approved",
  auditActionReject: "Rejected",
  auditActionAccept: "Accepted",
  auditActionDecline: "Declined",
  auditActionExpire: "Marked expired",
  auditActionRun: "Run",
  auditActionPause: "Paused",
  auditActionResume: "Resumed",
  auditActionRatesUpdate: "Exchange rate set",
  auditActionRatesImport: "Exchange rates imported",
  auditActionStageMove: "Moved to another column",
  auditActionStageCreate: "Column added",
  auditActionMove: "Moved",
  auditActionQuoteRaised: "Quote raised",
  auditActionInvoiceRaised: "Invoice raised",
  auditActionActivityCreate: "Note added",
  auditActionNextStepCreate: "Next step added",
  auditActionThreadCreate: "Conversation linked",
  auditActionThreadDelete: "Conversation unlinked",
  auditActionLeadCreate: "Leads imported",

  // Insights (alo Insights, ADR 0037, wave BI-1). The rail says "Insights" —
  // the module is where a business reads its own numbers, and no chart on it
  // is a figure the browser worked out.
  moduleInsights: "Insights",
  insightsBoards: "Boards",
  insightsLoadFailed: "Your boards could not be loaded.",
  insightsBoardLoadFailed: "This board could not be loaded.",
  insightsFiguresFailed: "These figures could not be read.",
  insightsSaveFailed: "The change could not be saved.",
  insightsDeleteFailed: "That could not be removed.",
  insightsNewBoard: "New board",
  insightsBoardNamePrompt: "What should this board be called?",
  insightsBoardNamePlaceholder: "Cash",
  insightsRenameBoard: "Rename",
  insightsDeleteBoard: "Delete board",
  insightsDeleteBoardConfirm: (name: string) =>
    `Delete the board “${name}”? Its charts go with it — the invoices and deals behind them stay.`,
  insightsRefresh: "Refresh the figures",
  insightsNoBoardsTitle: "No boards yet",
  insightsNoBoardsBody:
    "A board holds the numbers you want at a glance — what you billed, what you are owed, what is in the pipeline.",
  insightsNoTilesTitle: "Nothing pinned to this board",
  insightsNoTilesBody: "Charts pinned to this board appear here.",
  // The gallery of ready-made questions (BI1.06). The server sends a key per
  // entry and never a caption, so these words — and only these — are what a
  // reader sees, and what is stored as the tile's title when they pin one.
  insightsAddChart: "Add a chart",
  insightsGalleryTitle: "Ready-made charts",
  insightsGallerySubtitle:
    "Pick one to pin it to this board. You can rename or remove it after.",
  insightsGalleryClose: "Close",
  insightsGalleryLoadFailed: "The ready-made charts could not be loaded.",
  insightsGalleryRevenueByMonth: "Revenue by month",
  insightsGalleryRevenueByMonthBody:
    "What you invoiced, month by month, over the last year — excluding VAT.",
  insightsGalleryOutstanding: "Outstanding",
  insightsGalleryOutstandingBody:
    "Everything still owed to you on issued invoices, as one figure.",
  insightsGalleryOverdueAging: "Overdue by age",
  insightsGalleryOverdueAgingBody:
    "What is owed, grouped by how late it is: 0–30, 31–60, 61–90 and 90+ days.",
  insightsGalleryVatByQuarter: "VAT by quarter",
  insightsGalleryVatByQuarterBody:
    "VAT charged per quarter — the shape a return is filed in.",
  insightsGalleryTopCustomers: "Top customers",
  insightsGalleryTopCustomersBody:
    "Who this year's revenue came from, largest ten first.",
  insightsGalleryPaymentsByMonth: "Payments received",
  insightsGalleryPaymentsByMonthBody:
    "Money that actually arrived, month by month, in the currency it arrived in.",
  insightsGalleryPipelineByStage: "Pipeline by stage",
  insightsGalleryPipelineByStageBody:
    "The value of open deals in each column of your funnel.",
  insightsGalleryWonThisMonth: "Won this month",
  insightsGalleryWonThisMonthBody:
    "The value of deals closed as won this month.",
  insightsGalleryWinRateByQuarter: "Win rate by quarter",
  insightsGalleryWinRateByQuarterBody:
    "How often a decided deal was won, quarter by quarter.",
  insightsGalleryWonByMonth: "Won by month",
  insightsGalleryWonByMonthBody:
    "Deal value won, month by month over the last year.",
  insightsAsk: "Ask for a chart",
  insightsAskSubtitle:
    "Describe what you want to see. You get the chart to look at first — nothing is added to this board until you pin it.",
  insightsAskLabel: "Your question",
  insightsAskPlaceholder: "How much did we invoice each month this year?",
  insightsAskSubmit: "Ask",
  insightsAskClose: "Close",
  insightsAskPreview: "The proposed chart",
  insightsAskPin: "Pin to this board",
  insightsAskDiscard: "Discard",
  insightsAskRepaired:
    "The first attempt did not fit the data, so it was corrected before drawing.",
  insightsAskFailed: "No chart could be built from that question.",
  insightsAskUnavailable:
    "The assistant is not switched on for this workspace.",
  insightsTileActions: (title: string) => `Options for ${title}`,
  insightsRenameTile: "Rename chart",
  insightsRenameTilePrompt: "What should this chart be called?",
  insightsRemoveTile: "Remove chart",
  insightsRemoveTileConfirm: (title: string) =>
    `Remove “${title}” from this board? The records it counts are untouched.`,
  insightsWiden: "Make wider",
  insightsNarrow: "Make narrower",
  insightsMoveLeft: "Move earlier",
  insightsMoveRight: "Move later",
  insightsUnreadableTitle: "Made by a newer version of alo",
  insightsUnreadableBody:
    "This chart's question cannot be read here, so its figures are not shown.",
  insightsNoFigures: "Nothing to show for this period.",
  insightsTruncated:
    "Only the largest categories are shown; the rest are grouped as “Other”.",
  insightsNoteUnconverted: (count: number) =>
    count === 1
      ? "1 document could not be restated in your accounting currency and is not counted."
      : `${count} documents could not be restated in your accounting currency and are not counted.`,
  insightsColBucket: "Bucket",
  insightsColValue: "Value",
  insightsBucketTotal: "Total",
  insightsBucketOther: "Other",
  insightsGroupAll: "All",
  insightsValueNone: "None",
  insightsValueUnknown: "Unknown",
  insightsStatusIssued: "Issued",
  insightsStatusPaid: "Paid",
  insightsOutcomeWon: "Won",
  insightsOutcomeLost: "Lost",
  insightsOutcomeOpen: "Open",
  insightsAgeNotDue: "Not due",
  insightsAge0To30: "0–30 days",
  insightsAge31To60: "31–60 days",
  insightsAge61To90: "61–90 days",
  insightsAge90Plus: "90+ days",
  insightsQuarter: (quarter: number, year: number) => `Q${quarter} ${year}`,
  insightsWeek: (week: number, year: number) => `W${week} ${year}`,
  // Projects (alo Projects, ADR 0035, wave B3). The words of client work: an
  // engagement, the hours worked on it, the week they are handed in as, and the
  // decision somebody makes about that week.
  //
  // The rail reads "Projects" while Tasks also calls its boards projects —
  // they ARE the same rows, which is the point (docs/design/projects.md, "One
  // project list, extended"). So the copy here says "client project" wherever
  // the distinction carries weight and leaves the Tasks strings alone.
  //
  // Durations are written as a person says them ("7h 30m"), never as decimal
  // hours: "1.75" on one screen beside "1h 45m" on another is two numbers
  // somebody has to reconcile.
  moduleProjects: "Projects",
  projectsTabList: "All projects",
  projectsTabMyWork: "My work",
  projectsWorkspaceTasks: "Tasks",
  projectsTabWeek: "Timesheet",
  projectsTabApprovals: "Approvals",
  projectsTabReports: "Reports",
  projectsTabPlan: "Timeline",
  projectsLoadFailed: "Your projects could not be loaded.",
  projectsWorkspaceLoadFailed: "This project could not be opened.",
  projectsWorkspaceUnavailable: "Project unavailable",
  projectsRetry: "Try again",
  projectsSaveFailed: "The change could not be saved.",
  projectsStartFailed: "The timer could not be started.",
  projectsStopFailed: "The timer could not be stopped.",
  projectsCancel: "Cancel",
  projectsSave: "Save",
  projectsEdit: "Edit",
  projectsOpenProject: (name: string) => `Open ${name}`,
  projectsDetailsTitle: "Project details",
  projectsDetailsSubtitle:
    "Keep the outcome, timing, and current state clear for everyone.",
  projectsDescription: "Description",
  projectsStatus: "Status",
  projectsStatusPlanned: "Planned",
  projectsStatusActive: "Active",
  projectsStatusOnHold: "On hold",
  projectsStatusCompleted: "Completed",
  projectsStatusCancelled: "Cancelled",
  projectsTargetOn: "Target date",
  projectsDatesInvalid: "The target date cannot be before the start date.",
  projectsActions: "Actions",
  projectsNew: "New project",
  projectsNewTitle: "Create a project",
  projectsNewSubtitle: "Name the work and decide who it is for.",
  projectsName: "Project name",
  projectsNamePlaceholder: "For example, Website redesign",
  projectsWorkType: "This work is for",
  projectsClientWork: "A customer",
  projectsInternalWork: "Our company",
  projectsClientWorkHint: "Bill a customer for this work",
  projectsInternalWorkHint: "Keep this work internal",
  projectsNewCustomerHint:
    "You can add rates and budgets after creating the project.",
  projectsCreate: "Create project",
  projectsCreateFailed: "The project could not be created.",

  // Durations and rates. `projectsNoTime` is the dash an empty cell shows: a
  // blank cell reads as broken, a zero reads as work that took no time.
  projectsNoTime: "—",
  projectsHoursShort: (hours: number) => `${hours}h`,
  projectsMinutesShort: (minutes: number) => `${minutes}m`,
  projectsPerHour: (amount: string) => `${amount}/h`,
  projectsPercent: (percent: number) => `${percent}%`,
  projectsUnpriced: "Not priced",

  // The engagement list.
  projectsProject: "Project",
  projectsAllProjects: "All projects",
  projectsCustomer: "Customer",
  projectsCustomerHint: "The customer this project's hours are billed to.",
  projectsCustomerPick: "Choose a customer…",
  projectsNoCustomersAvailable:
    "No customers are available yet. Add one in Billing first.",
  projectsCustomerUnknown: "Unknown customer",
  projectsInternal: "Internal",
  projectsRate: "Hourly rate",
  projectsRateHint: "Left blank, the hours are counted but not priced.",
  projectsRateInvalid: "Write the rate as an amount, for example 95.00.",
  projectsHoursLogged: "Hours",
  projectsBillableHours: "Billable",
  projectsOfWhichBillable: (duration: string) => `${duration} billable`,
  projectsBudget: "Budget",
  projectsHealth: "Project health",
  projectsHealthOnTrack: "On track",
  projectsHealthAtRisk: "Needs attention",
  projectsHealthNeedsTarget: "Add a target date to make delivery risk visible.",
  projectsUpdates: "Project updates",
  projectsUpdatesSubtitle:
    "Share progress, decisions, and risks with everyone following this project.",
  projectsUpdateHealth: "Update health",
  projectsUpdateOffTrack: "Off track",
  projectsUpdatePlaceholder:
    "What changed? Add the outcome, decision, risk, or next step.",
  projectsUpdateHint:
    "Keep it concise and useful to someone catching up later.",
  projectsPublishUpdate: "Publish update",
  projectsUpdatesEmpty: "No updates yet",
  projectsUpdatesEmptyBody:
    "Publish the first update to give this project a durable narrative.",
  projectsUpdatesLoadFailed: "The project updates could not be loaded.",
  projectsUpdateSaveFailed: "The update could not be published.",
  projectsRemoveAttachment: "Remove attachment",
  projectsSomeone: "Someone",
  projectsBlockedTasks: (count: number) =>
    count === 1 ? "1 blocked task" : `${count} blocked tasks`,
  projectsOverdueTasks: (count: number) =>
    count === 1 ? "1 overdue task" : `${count} overdue tasks`,
  projectsWorkload: "Workload",
  projectsWorkloadEmpty: "No open work is assigned yet.",
  projectsOpenTasks: (count: number) =>
    count === 1 ? "1 open task" : `${count} open tasks`,
  projectsBudgetUsed: "Budget used",
  projectsBudgetHours: "Budget (hours)",
  projectsBudgetAmount: "Budget (amount)",
  projectsBudgetHint: "Advisory. Nothing stops an hour logged past it.",
  projectsBudgetHoursInvalid: "Write the budget as a whole number of hours.",
  projectsBudgetAmountInvalid:
    "Write the budget as an amount, for example 7600.00.",
  projectsLastWorked: "Last worked",
  projectsNeverWorked: "Never",
  projectsStartsOn: "Starts on",
  projectsMakeClientWork: "Make client work",
  projectsStartTimerOn: (project: string) => `Start the timer on ${project}`,
  projectsStartTimer: "Start timer",
  projectsEmptyTitle: "No projects yet",
  projectsEmptyBody:
    "Create a project for customer work or for your own company, then start tracking time.",

  // The engagement form.
  projectsClientSubtitle:
    "Who this project is worked for, and what an hour on it is worth.",
  projectsPersonalBoard:
    "This is a personal board. Only a team project can be client work — its hours are approved by somebody else and billed to a customer.",
  projectsDetach: "Make internal",
  projectsDetachTitle: "Make this internal work?",
  projectsDetachBody:
    "The hours stay exactly as they are. What goes is the claim that they are billable to a customer — and hours already on an invoice keep that invoice.",

  // The week grid.
  projectsPreviousWeek: "Previous",
  projectsNextWeek: "Next",
  projectsThisWeek: "This week",
  projectsWeekOf: (from: string, to: string) => `${from} – ${to}`,
  // How a handed-in week reads in the one approvals inbox (B6.07), where it
  // sits beside claims and time off and cannot rely on a column heading.
  projectsBillableOf: (hours: string) => `${hours} billable`,
  projectsWeek: "Week",
  projectsDay: "Day",
  projectsTask: "Task",
  projectsDuration: "Duration",
  projectsDurationHint:
    "90, 1:30 and 1,5 all mean an hour and a half. 2h means two hours.",
  projectsDurationInvalid:
    "Write a duration like 90, 1:30, 1,5 or 2h — up to one day.",
  projectsTotal: "Total",
  projectsAddRow: "Add a project row…",
  projectsBillable: "Billable to the customer",
  projectsNotBillable: "not billable",
  projectsNote: "Note",
  projectsNoNote: "No note",
  projectsNoteHint:
    "What you were doing. Nobody outside this workspace reads it.",
  projectsProposedEntry: "suggested",
  projectsBilledEntry: "on an invoice",
  projectsReadyToInvoice: "Ready to invoice",
  projectsReadyToInvoiceBody: (duration: string) =>
    `${duration} of approved time has not been invoiced yet.`,
  projectsWorkflowEyebrow: "Next step",
  projectsWorkflowLabel: "Project workflow",
  projectsWorkflowTasks: "Tasks",
  projectsWorkflowTime: "Time",
  projectsWorkflowApproval: "Approval",
  projectsWorkflowInvoice: "Invoice",
  projectsWorkflowTasksTitle: "Define the work",
  projectsWorkflowTasksBody:
    "Create the first task so the team knows what needs to happen next.",
  projectsWorkflowTimeTitle: "Record the work",
  projectsWorkflowTimeBody:
    "Log time against this project or its tasks while the work is fresh.",
  projectsWorkflowApprovalTitle: "Send the time for approval",
  projectsWorkflowApprovalBody:
    "Review the week and submit it so approved customer work can be invoiced.",
  projectsWorkflowAwaitingApprovalTitle: "Time is awaiting approval",
  projectsWorkflowAwaitingApprovalBody:
    "This time has already been submitted. Review the timesheet or wait for an approver before invoicing.",
  projectsWorkflowInvoiceTitle: "Turn approved work into an invoice",
  projectsWorkflowContinueTitle: "Keep the project moving",
  projectsWorkflowContinueBody: "Add the next time entry as work continues.",
  projectsReviewTimesheet: "Review timesheet",
  projectsCreateInvoice: "Create invoice",
  projectsCreateInvoiceSubtitle:
    "Choose the approved time to carry into a new draft invoice.",
  projectsInvoiceThrough: "Invoice through",
  projectsInvoiceCutoffHint:
    "Only approved, unbilled time through this date is included.",
  projectsNothingToInvoice: "Nothing ready to invoice",
  projectsNothingToInvoiceBody:
    "Approved time will appear here after the week is approved.",
  projectsUnratedTime: "No hourly rate is set for this time",
  projectsInvoiceRate: (rate: string) => `${rate} per hour`,
  projectsBelgianVat: "Belgian standard VAT is applied to this draft.",
  projectsCreateDraftInvoice: "Create draft invoice",
  projectsInvoiceLoadFailed: "The approved time could not be loaded.",
  projectsInvoiceCreateFailed: "The invoice draft could not be created.",
  projectsCellLabel: (project: string, day: string, duration: string) =>
    `${project}, ${day}: ${duration}`,
  projectsDeleteEntry: "Delete",
  projectsDeleteEntryTitle: "Delete these hours?",
  projectsDeleteEntryBody:
    "The entry goes for good. Its week has to be open for that.",
  projectsWeekEmptyTitle: "Nothing logged this week",
  projectsWeekEmptyBody:
    "Add your first time entry. Choose a project, enter the duration and note, and it will appear in this weekly review.",
  projectsWeekTitle: "Weekly timesheet",
  projectsWeekEntriesLabel: "Time entries this week",
  projectsWeekPurpose:
    "Log your work, review the week, then submit it for approval.",
  projectsWeekAllScope: "Your complete week across every project.",
  projectsWeekProjectScope: (project: string) =>
    `Time for ${project}. Submitting still sends your complete week for approval.`,
  projectsAddTime: "Add time",
  projectsChooseTimeProject: "What did you work on?",
  projectsChooseTimeProjectHint:
    "Choose a project to add a time entry for this week.",
  projectsBillableOfWeek: (duration: string) => `${duration} billable`,
  projectsCompleteWeek: "Entire week",
  projectsCompleteWeekSubmission: "Entire week submitted for approval",
  projectsProposedInWeek: (duration: string) =>
    `${duration} suggested, not yet accepted`,
  // Deciding about a suggestion (B3.10b). Accepting is what makes it an hour —
  // the wording says so, because "OK" would not.
  projectsAcceptEntry: "Accept",
  projectsRejectEntry: "Discard",
  projectsAcceptEntryLabel: (project: string, duration: string) =>
    `Accept the suggested ${duration} on ${project}`,
  projectsRejectEntryLabel: (project: string, duration: string) =>
    `Discard the suggested ${duration} on ${project}`,
  projectsSuggestionsWaiting: (count: number) =>
    count === 1
      ? "1 suggestion is waiting for you this week."
      : `${count} suggestions are waiting for you this week.`,
  projectsSubmitWeek: "Submit week",
  projectsWithdrawWeek: "Take it back",
  projectsRejectedBecause: (note: string) => `Sent back: ${note}`,

  // The plan — milestones on a date axis, over the board that already exists.
  // "Reached" is deliberately a person's word and not "complete": a milestone
  // is reached when somebody says the deliverable was accepted, never when the
  // last task under it was closed (docs/design/projects.md).
  projectsPlanLoadFailed: "The plan could not be loaded.",
  projectsMilestoneAdd: "Add a milestone",
  projectsMilestoneNew: "New milestone",
  projectsMilestoneName: "Milestone",
  projectsMilestoneNameHint:
    "What the date is for — \u201cDesign signed off\u201d, \u201cBeta with the pilot\u201d.",
  projectsMilestoneDue: "Date",
  projectsMilestoneDueHint:
    "The day it is due. Moving it later is ordinary; nothing is stopped by it.",
  projectsMilestoneReach: "Mark reached",
  projectsMilestoneReopen: "Not reached yet",
  projectsMilestoneReached: "Reached",
  projectsMilestoneLate: "Late",
  projectsMilestoneNoTasks: "No tasks under it yet",
  projectsMilestoneTasksClosed: (done: number, total: number) =>
    `${done} of ${total} tasks closed`,
  projectsMilestoneDelete: "Delete",
  projectsMilestoneDeleteTitle: "Delete this milestone?",
  projectsMilestoneDeleteBody:
    "The date goes; the tasks under it stay exactly where they are on the board.",
  projectsPlanUnplaced: "Not in the plan",
  projectsPlanPlace: "Put under\u2026",
  projectsPlanPlaceTask: (task: string) => `Put ${task} under a milestone`,
  projectsPlanRemove: "Take out",
  projectsPlanEmptyTitle: "No plan yet",
  projectsPlanEmptyBody:
    "A milestone is a named date on this project \u2014 the dates a client asks about. Add the first one, then put the board\u2019s tasks under it.",
  projectsTimelineAllEmptyTitle: "No milestones across your projects",
  projectsTimelineAllEmptyBody:
    "Choose a project above to add its first milestone, or keep this view on All projects for the portfolio timeline.",

  // Templates: a board marked reusable, and the copy started from it. The copy
  // says what travels and what does not, because a person about to start a
  // client's project needs to know before the board opens, not after.
  projectsTemplateNew: "New from template",
  projectsTemplateNewTitle: "Start from a template",
  projectsTemplateNewSubtitle: "The shape of the work, on new dates",
  projectsTemplateCreate: "Create project",
  projectsTemplateWhich: "Template",
  projectsTemplateWhichHint:
    "The cards, their columns, checklists and labels come along — not assignees, comments, hours or finished cards.",
  projectsTemplateOption: (name: string, tasks: number, milestones: number) =>
    `${name} — ${tasks} ${tasks === 1 ? "card" : "cards"}, ${milestones} ${
      milestones === 1 ? "milestone" : "milestones"
    }`,
  projectsTemplateName: "New project name",
  projectsTemplateNameHint: "What this one is called on the board.",
  projectsTemplateStarts: "Starts on",
  projectsTemplateStartsHint:
    "The template’s first milestone lands here; every other date keeps its spacing.",
  projectsTemplateCustomerHint:
    "A template is a shape, not a client. Leave it blank for internal work; the rate and budget come along either way.",
  projectsTemplateNoCustomer: "Internal work",
  projectsTemplateNoPlan:
    "This template has no milestones, so its dates are copied exactly as they are.",
  projectsTemplateMarkOn: (project: string) => `Make ${project} a template`,
  projectsTemplateUnmarkOn: (project: string) =>
    `${project} is a template — remove the mark`,
  projectsTemplateEmptyTitle: "No templates yet",
  projectsTemplateChooseProject: "Choose a project",
  projectsTemplateEmptyBody:
    "Open a project you would run the same way again and press the star beside it. It stays an ordinary board — it can just be copied.",
  projectsTemplateFailed: "That could not be done.",
  projectsTemplatesLoadFailed: "The templates could not be loaded.",

  // Where a week stands. The server's word, never re-derived in the browser.
  projectsWeekOpen: "Open",
  projectsWeekSubmitted: "Submitted",
  projectsWeekApproved: "Approved",
  projectsWeekRejected: "Sent back",

  // The approvals inbox — the one screen here that names a person.
  projectsPerson: "Person",
  projectsSubmittedAt: "Handed in",
  projectsApprove: "Approve",
  projectsApprovalComplete: "Week approved",
  projectsApprovalCompleteBody:
    "Review the affected projects and invoice any customer work that is ready.",
  projectsReject: "Send back",
  projectsRejectTitle: "Send this week back?",
  projectsRejectBody: (person: string) =>
    `${person} will read what you write here.`,
  projectsRejectPlaceholder: "What needs correcting",
  projectsApprovalsEmptyTitle: "Nothing to approve",
  projectsApprovalsEmptyBody: "Weeks people hand in land here, oldest first.",

  // The profitability report — hours × rates against a budget.
  //
  // The copy says *value* and never *margin*: this is the revenue side, and
  // what an hour costs us needs the ledger and the employee record neither of
  // which exists yet (docs/design/projects.md § Budgets). Two datings sit on
  // one screen and the basis line says so out loud: the work is the period's,
  // the budget is consumed by everything up to the period's last day.
  projectsReportTitle: "Profitability",
  projectsReportPortfolioTitle: "Portfolio report",
  projectsReportAllScope: "All client projects you can access.",
  projectsReportFrom: "From",
  projectsReportTo: "To",
  projectsReportShow: "Show",
  projectsReportThisQuarter: "This quarter",
  projectsReportLastQuarter: "Last quarter",
  projectsReportDownloadCsv: "Download CSV",
  projectsReportDownloadFailed: "The report could not be downloaded.",
  projectsReportBasis: (from: string, to: string) =>
    `Hours worked between ${from} and ${to}.`,
  projectsReportBudgetBasis: (to: string) =>
    `Budgets count everything up to ${to}, not just this period.`,
  projectsReportColValue: "Value",
  projectsReportColInvoiced: "Invoiced",
  projectsReportColToInvoice: "To invoice",
  projectsReportColToDate: "Hours to date",
  projectsReportColBudget: "Budget used",
  projectsReportTotals: "All engagements",
  projectsReportUnrated: (duration: string) => `${duration} not priced`,
  projectsReportUnratedHint:
    "Chargeable hours with no rate. They are counted here and valued nowhere — price the engagement, then log them.",
  projectsReportNoValue: "No value yet",
  projectsReportBudgetLeft: (amount: string) => `${amount} left`,
  projectsReportBudgetOver: (amount: string) => `${amount} over`,
  projectsReportNoBudget: "No budget set",
  projectsReportEmptyTitle: "No client projects yet",
  projectsReportEmptyBody:
    "Profitability is hours against a rate and a budget, so it starts with a client project. Give a project a customer and a rate, and this fills in.",

  // The running-timer widget in the rail.
  projectsTimerRunning: "Timer running",
  projectsStopTimer: "Stop the timer",
  projectsStop: "Stop",

  // ---- alo Finance (ADR 0035, wave B4) ---------------------------------
  //
  // The expenses slice (B4.13a): what somebody spent, and the three verbs that
  // settle it. The Bank, Accounts and Reports tabs are B4.13b/c and bring their
  // own strings.
  //
  // Two rules the copy follows. **Nothing here states an amount or a rule the
  // server owns**: a refusal is shown in the server's own sentence, and these
  // strings are only the fallback for a request that never reached it. And the
  // words are the ones a person uses about their own money — "paid back", not
  // "reimbursement processed" — because the person filling this in is an
  // employee with a receipt, not a bookkeeper.
  moduleFinance: "Finance",
  financeTabExpenses: "Expenses",
  financeTabApprovals: "Approvals",
  // The names `ds/Table` and `ds/Toolbar` require: a table is announced as
  // "table, seven columns" and a row of controls as nothing at all until each
  // says what it lists or acts on. Read, not drawn — every one of these screens
  // already carries the heading a sighted reader needs.
  financeClaimsTable: "Your claims",
  financeClaimFilters: "Claim filters",
  financeChartFilters: "Chart period",
  financeStatementsTable: "Imported statements",
  financeChartTableOf: (kind: string) => `Accounts — ${kind}`,
  financePendingClaimsTable: "Claims to decide on",
  financeOwedClaimsTable: "Claims to pay back",
  financeBankSampleTable: "Sample transactions",
  financeBankSettledTable: "Matched bank lines",
  financeBankSetAsideTable: "Bank lines set aside",
  financeBankFilters: "Statement filter",
  financeReportPeriod: "Report period",
  financeLoadFailed: "Your expense claims could not be loaded.",
  financeSaveFailed: "The change could not be saved.",
  financeCancel: "Cancel",
  financeSave: "Save",
  financeEdit: "Edit",
  financeDelete: "Delete",
  financeActions: "Actions",
  financeShow: "Show",
  financeFrom: "From",
  financeTo: "To",

  // The claim itself.
  financeNewClaim: "New claim",
  financeEditClaim: "Edit claim",
  financeClaimSubtitle: "What you spent, and whose money paid.",
  financeSpentOn: "Date",
  financeSpentOnHint: "The day the money left, in your own time zone.",
  financeMerchant: "Merchant",
  financeMerchantHint: "Who was paid — the name on the receipt.",
  financeNoMerchant: "No merchant",
  // How a waiting claim reads in the one approvals inbox (B6.07): what was
  // bought and the day it was bought, in one line.
  financeClaimOf: (merchant: string, day: string) => `${merchant}, ${day}`,
  financeDescription: "What it was for",
  financeGross: "Total",
  financeVat: "VAT",
  financeVatHint: "The VAT shown on the receipt. Leave empty if it shows none.",
  financeNoVat: "—",
  financeVatRate: "VAT rate %",
  financeVatRateHint: "As printed: 19, 21, 5.5.",
  financeCurrency: "Currency",
  financeCurrencyHint: "Leave empty for your workspace's own currency.",
  financeProject: "Project",
  financeProjectHint:
    "Attach the claim to client work, so it shows in that project's cost.",
  financeNoProject: "No project",
  financeMethod: "Paid with",
  financeMethodHint:
    "Your own money is the only one that ends in being paid back.",
  financeMethodPersonal: "Own money",
  financeMethodCard: "Company card",
  financeMethodCash: "Petty cash",
  financeMethodPersonalOption: "My own money",
  financeMethodCardOption: "The company card",
  financeMethodCashOption: "Petty cash",
  financeAmountInvalid: "That is not an amount.",
  financeRateInvalid: "That is not a percentage.",

  // Where a claim stands. The server's word for each, in the person's language.
  financeStatus: "Status",
  financeAnyStatus: "Any status",
  financeStatusDraft: "Draft",
  financeStatusSubmitted: "Waiting",
  financeStatusApproved: "Approved",
  financeStatusRejected: "Refused",
  financeStatusReimbursed: "Paid back",
  financePaidBackOn: (day: string) => `Paid back ${day}`,

  // The verbs.
  financeSubmit: "Hand in",
  financeWithdraw: "Take back",
  financeApprove: "Approve",
  financeReject: "Refuse",
  financeMarkPaidBack: "Mark paid back",
  financeMarkPaidBackSubtitle: (person: string, amount: string) =>
    `${amount} back to ${person}.`,
  financeReimbursedOn: "Paid back on",
  financeReimbursedOnHint:
    "The day the money actually moved — it is the day it books on.",
  financeDeleteTitle: "Delete this claim?",
  financeDeleteBody:
    "The claim and what you typed into it are removed. This cannot be undone.",
  financeRejectTitle: "Refuse this claim",
  financeRejectBody: (person: string) =>
    `${person} will read this, and can correct the claim and hand it in again.`,
  financeRejectPlaceholder: "Why it comes back…",

  // The approver's screen.
  financePerson: "Person",
  financeCategory: "Category",
  financeUncategorised: "Not classified",
  financeSubmittedAt: "Handed in",
  financeApprovedAt: "Approved",
  financeOfWhichVat: (amount: string) => `incl. ${amount} VAT`,
  financeWaitingTitle: "Waiting for a decision",
  financeWaitingEmptyTitle: "Nothing is waiting",
  financeWaitingEmptyBody:
    "Claims your colleagues hand in appear here, oldest purchase first.",
  financeOwedTitle: "To pay back",
  financeOwedNote:
    "Approved claims your colleagues paid out of their own pocket. A claim the company card paid is approved and owes nobody anything, so it is not here.",
  financeOwedEmptyTitle: "Nobody is owed anything",
  financeOwedEmptyBody:
    "Once you approve a claim somebody paid for themselves, it waits here until the money goes back.",

  // The first thing an employee sees of the module.
  financeExpensesEmptyTitle: "No claims in this period",
  financeExpensesEmptyBody:
    "Record what you spent for work — the date, the total on the receipt and whose money paid. It stays yours until you hand it in.",

  // ---- the bank, and the pile it leaves (B4.13b) -------------------------
  //
  // Two rules on top of the module's own. **Nothing here names a rule the
  // server owns**: a refusal — an unreadable row, a payment larger than the
  // debt, a line already matched — arrives as the server's own sentence, and
  // these strings are the fallback for a request that never reached it. And the
  // words are a bookkeeper's, not a programmer's: a file is a "statement", a
  // guess is what "we think", and CAMT.053 is spelled out because the person
  // downloading it from their bank has read that word on the download button.
  financeTabBank: "Bank",
  financeTabReconcile: "Match",
  financeBankLoadFailed: "The bank statements could not be loaded.",

  // Importing a statement.
  financeBankImportStatement: "Import a statement",
  financeBankImportTitle: "Import a bank statement",
  financeBankImportSubtitle:
    "We read the file first and show you what we made of it. Nothing is stored until you say so.",
  financeBankFile: "Statement file",
  financeBankFileHint:
    "A CAMT.053 or MT940 download from your bank, or a CSV export.",
  financeBankAccount: "Account",
  financeBankAccountHint:
    "The IBAN this statement is for. A CAMT.053 or MT940 file says it itself; a CSV does not.",
  financeBankCurrencyHint:
    "For a CSV that does not say. Leave empty for your workspace's own currency.",
  financeBankCheckFile: "Check this file",
  financeBankCheckAgain: "Check again",
  financeBankImport: "Import",
  financeBankReadFailed: "That file could not be read.",
  financeBankImportFailed: "Nothing was imported.",
  financeBankStale:
    "You changed how the file is read. Check it again to see the result.",
  financeBankStaged: (staged: number, duplicates: number) =>
    duplicates === 0
      ? `${staged} transactions imported.`
      : `${staged} transactions imported; ${duplicates} were already here and were left alone.`,

  // What the server made of the file.
  financeBankFormat: "Read as",
  financeBankSourceCamt: "CAMT.053",
  financeBankSourceMt940: "MT940",
  financeBankSourceCsv: "CSV",
  financeBankRows: "Transactions",
  financeBankRowsRead: (lines: number, rows: number) =>
    `${lines} of ${rows} rows`,
  financeBankSkipped: "Rows that are not transactions",
  financeBankUnbooked: "Not yet booked by the bank",
  financeBankPeriod: "Period",
  financeBankEncoding: "Encoding",
  financeBankSampleTitle: "The first transactions, as we read them",
  financeBankSampleTruncated:
    "Only the first transactions are shown here. All of them are imported.",
  financeBankRowsRefused: (count: number) =>
    count === 1
      ? "One row cannot be read, so nothing was imported."
      : `${count} rows cannot be read, so nothing was imported.`,
  financeBankRowAt: (line: number) => `Line ${line}:`,
  financeBankRowUnknown: "A row:",

  // Telling us which column is which.
  financeBankMappingTitle: "Which column is which",
  financeBankMappingNote:
    "We guessed from the file's own header. Correct anything we got wrong, then check the file again.",
  financeBankColumnNone: "Not in this file",
  financeBankColDate: "Booking date",
  financeBankColValueDate: "Value date",
  financeBankColAmount: "Amount (one signed column)",
  financeBankColDebit: "Money out",
  financeBankColCredit: "Money in",
  financeBankColSign: "Which way it points",
  financeBankColCurrency: "Currency per row",
  financeBankColCounterparty: "Who was paid, or who paid",
  financeBankColIban: "Their account",
  financeBankColRemittance: "What was written on the payment",
  financeBankColReference: "The bank's own reference",
  financeBankDates: "Dates read as",
  financeBankDecimal: "Cents separated by",
  financeBankConventionAuto: "Work it out from the file",
  financeBankConventionDmy: "Day/month/year",
  financeBankConventionMdy: "Month/day/year",
  financeBankConventionYmd: "Year-month-day",
  financeBankConventionComma: "A comma",
  financeBankConventionDot: "A dot",

  // What has been imported.
  financeBankLines: "Transactions",
  financeBankClosingBalance: "Closing balance",
  financeBankImportedAt: "Imported",
  financeBankEmptyTitle: "No statements yet",
  financeBankEmptyBody:
    "Import a month from your bank and every transaction in it lands in one pile, waiting to be matched to the invoices it paid.",

  // The reconciliation screen.
  financeBankStatement: "Statement",
  financeBankAllStatements: "Everything not yet matched",
  financeBankToMatchTitle: (count: number) =>
    count === 1 ? "1 transaction to match" : `${count} transactions to match`,
  financeBankAllMatchedTitle: "Nothing left to match",
  financeBankAllMatchedBody:
    "Every transaction in the imported statements is either attributed to an invoice or set aside. Import another month to carry on.",
  financeBankCapped:
    "This list is a first batch, not everything — work through it and reload to see the rest.",
  financeBankBookedOn: "Booked",
  financeBankCounterparty: "Who",
  financeBankNoCounterparty: "No name on the payment",
  financeBankRemittance: "Reference",
  financeBankCertain: "Certain",
  financeBankThisOne: "This one",
  financeBankNoGuess:
    "We have no idea what this one is. Pick the invoice, or set it aside.",
  financeBankNotOurs: "Not ours",
  financeBankPickInvoice: "Pick an invoice",
  financeBankStillOwed: "still owed",
  financeBankStillOwedIs: (amount: string) => `${amount} still owed`,
  financeBankMatchFailed: "That transaction was not attributed.",
  financeBankUnmatchFailed: "That match was not taken back.",
  financeBankIgnoreFailed: "That transaction was not set aside.",

  // Why we think a transaction settled a document. The server sends the fact;
  // these are the sentences it is read as.
  financeBankWhyNumberQuoted: "our invoice number is written on the payment",
  financeBankWhyRuleSaved: "this payer has been matched this way before",
  financeBankWhyCustomerNamed: (percent: number) =>
    `the name on the payment looks like the customer's (${percent}%)`,
  financeBankWhyWholeAmount: "the amount is exactly what is owed",
  financeBankWhyOnlyDocument: "it is the only open invoice for this amount",
  financeBankWhyBeforeDue: (days: number) =>
    days === 1
      ? "it arrived the day before it was due"
      : `it arrived ${days} days before it was due`,
  financeBankWhyAfterDue: (days: number) =>
    days === 1
      ? "it arrived the day after it was due"
      : `it arrived ${days} days after it was due`,
  financeBankWhyPartPayment: (amount: string) =>
    `it is part of the invoice — ${amount} would be left`,

  // Setting a transaction aside.
  financeBankIgnoreTitle: "Not ours to book",
  financeBankIgnoreBody:
    "Say why, so the next person reading this statement does not have to work it out again. Bank charges, a private transfer, a duplicate.",
  financeBankIgnore: "Set aside",
  financeBankIgnorePlaceholder: "Why it is not ours…",

  // Picking the invoice by hand.
  financeBankPickTitle: "Which invoice did this settle?",
  financeBankPickSubtitle: (amount: string) =>
    `${amount} arrived. Say what it paid.`,
  financeBankFindInvoice: "Find an invoice",
  financeBankFindInvoiceHint:
    "By number, or by the reference your customer gave it.",
  financeBankNoOpenInvoices: "No issued invoice is still waiting for money.",
  financeBankNoNumber: "No number",
  financeBankOverdue: "Overdue",
  financeBankConfirmMatch: "This one settled it",

  // What is already dealt with.
  financeBankUnmatched: "To match",
  financeBankMatched: "Matched",
  financeBankIgnored: "Set aside",
  financeBankSettledTitle: "Already matched",
  financeBankSettledNote:
    "Each of these recorded a payment and moved the books. Taking one back reverses it with an entry of its own.",
  financeBankUndoMatch: "Take it back",
  financeBankSetAsideTitle: "Set aside",
  financeBankSetAsideNote:
    "Transactions somebody decided are not ours to book.",
  financeBankUndoIgnore: "Back to the pile",

  // ---- alo Finance: the chart of accounts (B4.13c) ------------------------
  //
  // The words here have one job the rest of the module does not: they have to
  // make a double-entry chart editable by somebody who is not an accountant.
  // So a role is offered as the sentence it means ("what customers owe us"),
  // never as the word the wire uses, and the two rules that actually matter —
  // the posting rules follow the role and not the code, and an account with
  // history is retired rather than deleted — are said where somebody is about
  // to need them rather than in a manual nobody opens.
  financeTabAccounts: "Accounts",
  financeChartLoadFailed: "The chart of accounts could not be loaded.",
  financeChartSeeded:
    "We started you off with a neutral chart of accounts. Every one of these is yours to rename or renumber — your accountant's numbering will not break anything, because the bookkeeping follows each account's job and not its number.",
  financeChartEmptyTitle: "No accounts yet",
  financeChartEmptyBody:
    "The chart of accounts is the list of places money can be: the bank, what customers owe you, what you earn, what you spend. Nothing can be booked until there is one.",

  financeAccountAdd: "Add an account",
  financeAccountEdit: "Edit",
  financeAccountDelete: "Delete",
  financeAccountCode: "Number",
  financeAccountCodeHint:
    "What your accountant calls it. Letters and digits, no spaces.",
  financeAccountName: "Name",
  financeAccountRole: "Job",
  financeAccountRoleHint:
    "What this account is used for automatically. Invoices, payments and claims find their account by its job, never by its number — so renumbering is safe, and taking a job away stops those documents booking until another account has it.",
  financeAccountType: "Kind",
  financeAccountTypeHint:
    "What the account holds. It decides which report the account appears on.",
  financeAccountTypeUnset: "Choose one…",
  financeAccountActive: "In use",
  financeAccountActiveHint:
    "A retired account keeps its history and its balance and stops being offered on new documents.",
  financeAccountInUse: "In use",
  financeAccountRetired: "Retired",
  financeAccountShowRetired: "Show retired",
  financeAccountMovement: "Movement",
  financeAccountPostings: "Entries",
  financeAccountSystemNote:
    "We created this account, so it cannot be deleted — the bookkeeping resolves through it. Rename it, renumber it, or retire it.",
  financeAccountNewTitle: "Add an account",
  financeAccountNewBody: "Your own line in your own chart.",
  financeAccountEditTitle: "Edit the account",
  financeAccountEditBody: "Renaming and renumbering are safe at any time.",
  financeAccountSaveFailed: "The account was not saved.",
  financeAccountDeleteFailed: "The account was not deleted.",

  // The five kinds, twice: the short word for a table heading, and the sentence
  // somebody choosing one is actually answering.
  financeAccountTypeAsset: "What we own",
  financeAccountTypeLiability: "What we owe",
  financeAccountTypeEquity: "Equity",
  financeAccountTypeIncome: "What we earn",
  financeAccountTypeExpense: "What we spend",
  financeAccountTypeAssetLong:
    "Something we own or are owed — a bank account, cash, customers' debts",
  financeAccountTypeLiabilityLong:
    "Something we owe — suppliers, tax, money owed to staff",
  financeAccountTypeEquityLong:
    "The owners' stake, and the balances the books opened with",
  financeAccountTypeIncomeLong: "Something we earn",
  financeAccountTypeExpenseLong: "Something we spend",

  // The jobs a posting rule resolves through, each said as what it is for.
  financeRoleNone: "No particular job",
  financeRoleAr: "What customers owe us",
  financeRoleAp: "What we owe suppliers",
  financeRoleBank: "The bank account money moves through",
  financeRoleCash: "Petty cash",
  financeRoleVatOutput: "VAT we charged and owe",
  financeRoleVatInput: "VAT we paid and can reclaim",
  financeRoleRevenue: "Sales revenue",
  financeRoleExpenseDefault: "Costs with no category of their own",
  financeRoleEmployeePayable: "Expense claims we owe staff",
  financeRoleFxDiff: "Exchange differences",
  financeRoleRounding: "Rounding differences",
  financeRoleOpeningBalance: "The balances the books opened with",
  financeRoleSuspense: "Money we cannot place yet",

  // ---- alo Finance: the four reports (B4.13c) -----------------------------
  //
  // Every figure on these screens is the server's fold of the journal, in
  // integer cents; nothing here is a total a browser added up, and no heading
  // names a period the server did not state. The words are a business owner's
  // where they can be ("What we own") and an accountant's where they must be
  // ("Equity"), because a balance sheet is read by both.
  financeTabReports: "Reports",
  financeReportPl: "Profit and loss",
  financeReportBalance: "Balance sheet",
  financeReportAged: "Who owes what",
  financeReportVat: "VAT return",
  financeReportFrom: "From",
  financeReportTo: "To",
  financeReportOn: "On",
  financeReportShow: "Show",
  financeReportToday: "Today",
  financeReportThisYear: "This year",
  financeReportThisQuarter: "This quarter",
  financeReportLastQuarter: "Last quarter",
  financeReportLastYearEnd: "End of last year",
  financeReportDownloadCsv: "Download CSV",
  financeReportDownloadFailed: "The file could not be downloaded.",
  financeReportLoadFailed: "The report could not be loaded.",
  financeReportBasis: (from: string, to: string) =>
    `Everything booked between ${from} and ${to}, both days included.`,
  financeReportBasisOn: (on: string) =>
    `Everything booked up to and including ${on}.`,
  financeReportEmptyTitle: "Nothing booked yet",
  financeReportEmptyBody:
    "Issued invoices, payments and approved expense claims book themselves. As soon as one does, it shows up here.",
  financeReportAmount: "Amount",
  financeReportTotal: "Total",
  financeReportPrevious: (from: string, to: string) => `${from} – ${to}`,

  // The profit and loss.
  financeReportIncome: "What we earned",
  financeReportIncomeTotal: "Earned in total",
  financeReportExpense: "What we spent",
  financeReportExpenseTotal: "Spent in total",
  financeReportProfit: "Profit",
  financeReportLoss: "Loss",

  // The balance sheet.
  financeReportAssets: "What we own",
  financeReportAssetsTotal: "Owned in total",
  financeReportLiabilities: "What we owe",
  financeReportLiabilitiesTotal: "Owed in total",
  financeReportEquity: "Equity",
  financeReportEquityTotal: "Equity in total",
  financeReportResultToDate:
    "Profit or loss so far, not yet closed into equity",
  financeReportLiabilitiesEquityTotal: "Owed, equity and result together",
  financeReportDifference: "Difference",
  financeReportUnbalanced: (amount: string) =>
    `These books do not balance: ${amount} is unaccounted for. Do not file anything from this sheet — send it to us instead.`,

  // Who owes what.
  financeReportSide: "Showing",
  financeReportReceivable: "What we are owed",
  financeReportPayable: "What we owe",
  financeReportParty: "Who",
  financeReportBandCurrent: "Not yet due",
  financeReportBand1To30: "1–30 days",
  financeReportBand31To60: "31–60 days",
  financeReportBand61To90: "61–90 days",
  financeReportBand90Plus: "Over 90 days",
  financeReportOpenDocuments: (count: number) =>
    count === 1 ? "1 open document" : `${count} open documents`,
  financeReportNothingOwedToUs: "Nobody owes you anything",
  financeReportNothingWeOwe: "You owe nobody anything",
  financeReportAgedEmptyBody:
    "Every issued document on this side has been settled in full.",
  financeReportUnconverted: (count: number) =>
    count === 1
      ? "1 document is in none of these columns: we have no exchange rate to state it in your own currency."
      : `${count} documents are in none of these columns: we have no exchange rate to state them in your own currency.`,

  // The VAT return.
  financeReportVatRate: "Rate",
  financeReportVatBase: "Amount before VAT",
  financeReportVatTax: "VAT",
  financeReportVatOutput: "VAT we charged",
  financeReportVatOutputTotal: "Charged in total",
  financeReportVatInput: "VAT we paid",
  financeReportVatInputTotal: "Paid in total",
  financeReportVatUnrated: "On no stated rate",
  financeReportVatPayable: "To pay",
  financeReportVatRefund: "To reclaim",
  financeReportVatNote:
    "These are your books' figures — sales and purchases both — which is what a return is filed from. The VAT summary under Billing shows what you invoiced, which is a different question.",

  // ---- alo Inventory (ADR 0035, wave B5.09a) -------------------------------
  //
  // The catalog and the stock list, and the movement history behind a row.
  //
  // Three rules the copy follows. **Nothing here states a quantity, a value or
  // a rule the server owns**: every figure on these screens is the ledger's
  // fold, a refusal is shown in the server's own sentence, and these strings
  // are only the fallback for a request that never reached it. **A value is
  // never called a balance** — B5 chooses no costing method, so what a screen
  // shows is what the goods cost us at today's purchase price, and the words
  // say exactly that. And the words are a warehouse's — "on hand", "what we
  // pay" — because the person reading them is holding a box, not closing a
  // ledger.
  moduleInventory: "Inventory",
  inventoryTabCatalog: "Catalog",
  inventoryTabStock: "Stock",
  inventoryLoadFailed: "Your catalog could not be loaded.",
  inventorySaveFailed: "The change could not be saved.",
  inventoryHistoryFailed: "That history could not be loaded.",
  inventoryClose: "Close",
  inventoryEdit: "Edit",
  inventoryArchive: "Archive",
  inventoryRestore: "Restore",
  inventoryArchived: "archived",
  inventoryColActions: "Actions",
  inventoryNoMatches: "Nothing here matches what you typed.",

  // The catalog: the price list seen as things.
  inventoryNewProduct: "New product",
  inventorySearchCatalog: "Search by name, code or barcode",
  inventoryStockedOnly: "Stocked only",
  inventoryShowArchived: "Show archived",
  inventoryCatalogEmptyTitle: "Your catalog is empty",
  inventoryCatalogEmptyBody:
    "A product here is one record: what you charge for it, what you pay for it, and — if it is something you keep on a shelf — how much of it you have. Add the first one and it can go on an invoice and into a warehouse the same day.",
  inventoryColProduct: "Product",
  inventoryColSku: "Code",
  inventoryColBarcode: "Barcode",
  inventoryColOnHand: "On hand",
  inventoryColPurchasePrice: "We pay",
  inventoryColSalePrice: "We charge",
  inventoryColVatRate: "VAT",
  inventoryTypeStocked: "Stocked",
  inventoryTypeService: "Service",
  inventoryNotStocked: "—",
  inventoryArchiveProductConfirm: (name: string) =>
    `Archive ${name}? It stays on every document already raised from it and stops being offered on new ones. You can restore it at any time.`,

  // The catalog fields on the product form, which Billing's price list and this
  // module's catalog share. The two hints that matter are the ones about rules
  // the server enforces: a barcode's check digit, and what "stocked" decides.
  inventoryFieldSku: "Code (SKU)",
  inventorySkuHint:
    "Your own code for this item. Unique among your products; leave it empty if you have none.",
  inventoryFieldBarcode: "Barcode",
  inventoryBarcodeHint:
    "The GTIN on the box. Its check digit is verified, so a mistyped code is refused here rather than found when the wrong thing ships.",
  inventoryFieldPurchasePrice: "Purchase price",
  inventoryPurchasePriceHint: "What you pay for it, in your own currency.",
  inventoryFieldDefaultSupplier: "Usual supplier",
  inventoryDefaultSupplierHint:
    "Who this is normally bought from. It is what a reorder proposal starts from.",
  inventoryNoSupplier: "Nobody in particular",
  inventoryFieldStocked: "Stock",
  inventoryStockedLabel: "Keep a quantity of this",
  inventoryStockedHint:
    "Only a stocked product can move between places. A service cannot be received, delivered or counted — and once something has moved, this cannot be turned off again.",

  // The stock list, and what its figures mean.
  inventorySearchStock: "Search by product, code or place",
  inventoryFilterLocation: "Place",
  inventoryAllLocations: "Everywhere",
  inventoryShowCounterparties: "Show counterparties",
  inventoryCounterpartiesNote:
    "Suppliers, customers, adjustments and production are counterparties, not places: they are the other end of every movement. With them shown, the total below sums to roughly nothing — which is what a ledger that closes looks like, not an empty warehouse.",
  inventoryStockEmptyTitle: "Nothing is on the shelves yet",
  inventoryStockEmptyBody:
    "Stock appears here when something moves: a purchase order you receive, a delivery you send, or an adjustment you make by hand. There is no quantity to type — what is here is the sum of everything that has happened.",
  inventoryColLocation: "Place",
  inventoryColValue: "Value",
  inventoryColLastMove: "Last movement",
  inventoryOpenHistory: "History",
  inventoryReferenceValue: (total: string) =>
    `${total} at today's purchase prices — a reference figure for what is listed, not an accounting balance.`,

  // The movement history: from → to, how many, why, and which document.
  inventoryHistoryTitle: (product: string) => `${product} — movements`,
  inventoryHistorySubtitle: (place: string) =>
    `Everything that moved in or out of ${place}.`,
  inventoryHistoryEmpty: "Nothing has moved in or out of this place yet.",
  inventoryHistoryCapped: (limit: number) =>
    `Showing the most recent ${limit} movements. Older ones are still recorded.`,
  inventoryColWhen: "When",
  inventoryColMovement: "From → to",
  inventoryColQuantity: "Quantity",
  inventoryColWhy: "Why",
  inventoryColDocument: "Document",
  inventoryNoDocument: "By hand",

  // What a place is. The four counterparties are named as what they mean to a
  // warehouse rather than as the words the wire uses.
  inventoryKindStock: "Warehouse",
  inventoryKindTransit: "In transit",
  inventoryKindSupplier: "Supplier",
  inventoryKindCustomer: "Customer",
  inventoryKindAdjust: "Adjustment",
  inventoryKindProduction: "Production",

  // Why something moved.
  inventoryReasonReceipt: "Received",
  inventoryReasonDelivery: "Delivered",
  inventoryReasonTransfer: "Transferred",
  inventoryReasonAdjustment: "Adjusted",
  inventoryReasonReturn: "Returned",
  inventoryReasonShrinkage: "Shrinkage",
  inventoryReasonCount: "Stocktake",

  // The reason somebody gave for an adjustment they made by hand.
  inventoryAdjustDamaged: "Damaged",
  inventoryAdjustLost: "Lost",
  inventoryAdjustFound: "Found",
  inventoryAdjustExpired: "Expired",
  inventoryAdjustTheft: "Theft",
  inventoryAdjustSample: "Sample",
  inventoryAdjustCorrection: "Correction",

  // ---- the two order documents (B5.09b) ------------------------------------
  //
  // Purchasing and sales orders. The copy follows the same three rules as the
  // rest of this module, plus one the documents make necessary: **a sentence
  // that precedes an irreversible act says what it will do, not "are you
  // sure"**. Placing an order draws a number from a gapless series and writes a
  // letter; booking an arrival moves real goods and raises a bill. A person who
  // reads the words should be able to predict the consequence exactly, because
  // there is no undo for any of them.
  inventoryTabPurchasing: "Purchasing",
  inventoryTabSales: "Sales orders",
  inventoryOrdersLoadFailed: "Those orders could not be loaded.",
  inventoryOrderLoadFailed: "That order could not be loaded.",
  inventoryDraftOrder: "Draft",
  inventoryDraftInvoice: "Draft invoice",
  inventoryOrderLate: "Late",
  inventoryFilterStatus: "State",
  inventoryAllStatuses: "Any state",
  inventoryNoOrdersInState: "No orders in that state",
  inventoryCancelAction: "Cancel",

  // What a state is called. "Cancelled" is shared: an order given up on is
  // given up on, whichever way the goods were going.
  inventoryOrderStatusCancelled: "Cancelled",
  inventoryPoStatusDraft: "Draft",
  inventoryPoStatusSent: "Placed",
  inventoryPoStatusPartial: "Part received",
  inventoryPoStatusReceived: "Received",
  inventorySoStatusDraft: "Draft",
  inventorySoStatusConfirmed: "Confirmed",
  inventorySoStatusPartial: "Part delivered",
  inventorySoStatusDelivered: "Delivered",

  // The two lists.
  inventorySearchPurchaseOrders: "Search by number, supplier or reference",
  inventorySearchSalesOrders: "Search by number, customer or reference",
  inventoryNewPurchaseOrder: "New purchase order",
  inventoryNewSalesOrder: "New sales order",
  inventoryPurchaseOrdersEmptyTitle: "You have not ordered anything yet",
  inventoryPurchaseOrdersEmptyBody:
    "A purchase order records what you asked a supplier for. Raise one as a draft, place it when you are ready, and book what arrives against it — the stock ledger is written for you.",
  inventorySalesOrdersEmptyTitle: "No customer has ordered anything yet",
  inventorySalesOrdersEmptyBody:
    "A sales order records what a customer asked you for. Raise one as a draft, confirm it to give it a number, and book each consignment as it goes out — the invoice bills what has actually gone.",
  inventoryColOrder: "Order",
  inventoryColSupplier: "Supplier",
  inventoryColCustomer: "Customer",
  inventoryColExpected: "Expected",
  inventoryColPromised: "Promised",
  inventoryColState: "State",
  inventoryColTotal: "Total",

  // The order book (O1.d) — one screen for what is promised, what has gone out,
  // what has been billed and what is still owed. `inventoryColDelivered` and
  // `inventoryColOutstanding` are the line grid's own and are reused here on
  // purpose: the same word must mean the same thing on both screens.
  inventoryTabOrderBook: "Order book",
  inventoryOrderBookLoadFailed: "The order book could not be loaded.",
  inventoryFilterScope: "Show",
  inventoryScopeOpen: "Open orders",
  inventoryScopeAll: "Every order",
  inventoryColOrdered: "Ordered",
  inventoryColReserved: "Reserved",
  inventoryColInvoiced: "Invoiced",
  inventoryBookTotal: "Across all of them",
  // Said rather than shown: adding euros to pounds gives a number that looks
  // authoritative and means nothing, so the total is withheld and the reason
  // given.
  inventoryBookMixedCurrencies: (currencies: string) =>
    `These orders are in ${currencies}, so there is no single total to show. Each order's own figures are exact.`,
  inventoryBookQtyHint: (qtyMilli: string) => `${qtyMilli} still to go out`,
  inventoryOrderBookEmptyTitle: "Nothing is outstanding",
  inventoryOrderBookEmptyBody:
    "The order book shows what customers are waiting for and what you have yet to bill them for. Confirm a sales order and it appears here until the last of it has gone out and been invoiced.",
  inventoryOrderBookEmptyAllTitle: "No orders have been raised",
  inventoryOrderBookEmptyAllBody:
    "Nothing has been sold yet — not even a draft. The order book will fill itself in as orders are raised.",

  // The document.
  inventoryBackToPurchaseOrders: "All purchase orders",
  inventoryBackToSalesOrders: "All sales orders",
  inventoryCreateDraft: "Create draft",
  inventorySaveDraft: "Save",
  inventoryPrintOrder: "Print",
  inventoryUnsavedNotice:
    "These changes are not saved yet, so the totals below are the last ones the server worked out.",
  inventoryOrderFrozenNotice:
    "This order has been placed. It carries a number the supplier holds, so it can no longer be edited — book what arrives against it, or cancel it.",
  inventorySalesOrderFrozenNotice:
    "This order has been confirmed. It carries a number the customer holds, so it can no longer be edited — book each consignment as it goes out.",
  inventoryFixLinesFirst:
    "One of the lines is not finished. Fix it and save again.",
  inventoryOrderNeedsSupplier: "Choose the supplier this order is placed with.",
  inventoryOrderNeedsCustomer: "Choose the customer this order is for.",
  inventoryPickSupplier: "Choose a supplier",
  inventoryPickCustomer: "Choose a customer",
  inventorySupplierHint:
    "Who you are ordering from. It cannot be changed once the order is placed.",
  inventoryCustomerHint:
    "Who the order is for. It cannot be changed once the order is confirmed.",
  inventoryExpectedHint:
    "The day you expect the goods. An order past it is flagged as late.",
  inventoryPromisedHint:
    "The day you promised the goods. An order past it is flagged as late.",
  inventoryFieldReference: "Reference",
  inventoryReferenceHint:
    "Your own reference for this order — a project, a site, a job number.",
  inventoryFieldOrdered: "Placed",
  inventoryFieldConfirmed: "Confirmed",
  inventoryFieldNote: "Note",
  inventoryOrderNoteHint:
    "Anything the other side should read. It is printed on the order.",

  // The line grid. The words are a document's, because these lines become one.
  inventoryLines: "Lines",
  inventoryAddLine: "Add line",
  inventoryNoLines: "No lines yet.",
  inventoryColDescription: "Description",
  inventoryColUnit: "Unit",
  inventoryColUnitPrice: "Unit price",
  inventoryColNet: "Net",
  inventoryColReceived: "Received",
  inventoryColDelivered: "Delivered",
  inventoryColOutstanding: "Outstanding",
  inventoryColToBill: "To bill",
  inventoryPickProduct: "From the catalog",
  inventoryDescriptionPlaceholder: "What is being ordered",
  inventoryUnitPlaceholder: "piece",
  inventoryQtyPlaceholder: "1",
  inventoryAmountPlaceholder: "0.00",
  inventoryRatePlaceholder: "0",
  inventoryRemoveLine: "Remove line",
  inventoryLineNeedsDescription: "Say what this line is for.",
  inventoryNotAQuantity: "That is not a quantity.",
  inventoryNotAnAmount: "That is not an amount.",
  inventoryNotARate: "That is not a rate.",

  // Placing an order: one act, and the words say all three parts of it.
  inventorySendOrder: "Place order",
  inventorySendOrderConfirm:
    "This gives the order its number, freezes it for good, and writes the covering letter with the printed order attached to your Drafts. Nothing is sent until you send it yourself.",
  inventoryOrderPlacedNotice: (to: string, file: string) =>
    `The order is placed. A covering letter to ${to} with ${file} attached is waiting in your Drafts — nothing has been sent.`,
  inventoryConfirmOrder: "Confirm order",
  inventoryConfirmOrderConfirm:
    "This gives the order its number and freezes it for good. It writes no message: telling the customer is an ordinary letter you send yourself.",
  inventoryCancelOrder: "Cancel order",
  inventoryCancelOrderConfirm:
    "The order is kept and stays readable, but nothing more is expected against it.",
  inventoryCancelShortConfirm:
    "Some of this order has already moved. Cancelling it accepts what has been handled so far as the whole of it, and nothing more will be expected. The order stays readable.",
  inventoryDiscardDraft: "Discard draft",
  inventoryDiscardDraftConfirm:
    "This draft has no number and has been shown to nobody, so it is deleted rather than cancelled.",

  // Booking a consignment, either direction.
  inventoryReceiveGoods: "Book arrival",
  inventoryDeliverGoods: "Book consignment",
  inventoryReceiveTitle: (order: string) => `What arrived against ${order}`,
  inventoryDeliverTitle: (order: string) =>
    `What is going out against ${order}`,
  inventoryReceiveSubtitle:
    "Each line opens on what is still outstanding. Change what you are short of; the rest stays on order. A draft bill is raised for what arrived.",
  inventoryDeliverSubtitle:
    "Each line opens on what is still outstanding. Change what is going now; the rest stays on the order.",
  inventoryReceiveWhere: "Put away at",
  inventoryReceiveWhereHint:
    "Where the goods were actually put. The stock ledger is written against this place.",
  inventoryDeliverWhere: "Picked from",
  inventoryDeliverWhereHint:
    "Where the goods were picked from. The stock ledger is written against this place.",
  inventoryColThisConsignment: "This time",
  inventoryFulfilNoteHint:
    "What the person handling it wrote — a damaged crate, a part shipment.",
  inventoryFulfilNeedsPlace: "Choose the place first.",
  inventoryFulfilNeedsSomething:
    "Nothing is stated on any line, so there is nothing to book.",
  inventoryNoPlaces: "No places yet",
  inventoryBookArrival: "Book it in",
  inventoryBookConsignment: "Book it out",
  inventoryArrivalBooked:
    "The arrival is booked, the stock ledger is written, and a draft bill is waiting for approval.",
  inventoryConsignmentBooked:
    "The consignment is booked and the stock ledger is written.",

  // What has already moved, and what has been billed for it.
  inventoryArrivals: "Arrivals",
  inventoryNoArrivals: "Nothing has arrived against this order yet.",
  inventoryArrivalNo: (n: number) => `Arrival ${n}`,
  inventoryBillDrafted: "Bill drafted",
  inventoryConsignments: "Consignments",
  inventoryNoConsignments: "Nothing has gone out against this order yet.",
  inventoryConsignmentNo: (n: number) => `Consignment ${n}`,
  inventoryRaiseInvoice: "Invoice what has gone",
  inventoryRaisedInvoices: "Invoices",
  inventoryNoRaisedInvoices: "Nothing has been invoiced from this order yet.",
  inventoryInvoiceDrafted:
    "A draft invoice has been raised for what has gone out. It carries no number until somebody issues it in Billing.",

  // ---- scanning (B5.09c) ----------------------------------------------------
  //
  // The words follow the hardware. A keyboard-wedge scanner is a keyboard, so
  // the field is the headline and the copy tells a person they can simply
  // scan into it; the camera is named as a second way and offered only where
  // the browser has one, because a button that appears and then apologises is
  // worse than no button. Nothing here explains what a barcode is: the
  // sentences a person actually needs — the check digit, the length — are the
  // server's, and they are shown verbatim.
  inventoryScan: "Scan",
  inventoryScanTitle: "Scan a barcode",
  inventoryScanSubtitle:
    "Scan into the field with a handheld reader, or type the code. On a phone you can use the camera instead.",
  inventoryScanFieldCode: "Barcode",
  inventoryScanPlaceholder: "4006381333931",
  inventoryScanHint:
    "A handheld scanner types the code here and presses Enter for you. Spaces and hyphens are ignored.",
  inventoryScanLookup: "Find it",
  inventoryScanFailed: "That code could not be looked up.",
  inventoryScanWaiting: "Waiting for a code.",
  inventoryScanCameraStart: "Use the camera",
  inventoryScanCameraStop: "Stop the camera",
  inventoryScanCameraFailed:
    "The camera could not be started. Allow access to it, or type the code — a handheld scanner needs no permission at all.",
  inventoryScanAiming:
    "Point the camera at the barcode. It stops as soon as it reads one.",
  inventoryScanNoCamera:
    "This browser cannot read a barcode from a camera. A handheld scanner works here: it types into the field above.",
  inventoryScanOnHand: (quantity: string) =>
    `${quantity} on hand, across every place.`,
  inventoryScanNowhere: "None of it is anywhere yet.",
  inventoryScanServiceNote:
    "This is a service, so there is no quantity of it to find.",
  inventoryScanOpenProduct: "Open this product",
  inventoryScanShowInStock: "Show it in the list",
  inventoryScanAddProduct: "Add it to the catalog with this barcode",

  mailAttachmentErrorDetail: (reason: string) =>
    `That file was not attached. Try adding it again. Server: ${reason}`,
  mailDraftCreateErrorDetail: (reason: string) =>
    `Your message was not sent because its draft could not be created. The compose window is still open; try Send again. Server: ${reason}`,
  mailSubmitErrorDetail: (reason: string) =>
    `Your message was not sent. It remains in Drafts so you can open it and try again. Server: ${reason}`,
  mailScheduleErrorDetail: (reason: string) =>
    `Your message was not scheduled. It remains in Drafts so you can open it and try again. Server: ${reason}`,

  // ---- alo HR (ADR 0035, wave B6) --------------------------------------
  //
  // The words of a module that holds people's records rather than a company's
  // documents, and three rules follow from that. **Nothing here judges a
  // person**: there is no word for a score, a rank, a fit or a shortlist,
  // because none of those is a thing this product computes and the vocabulary
  // is where that promise is kept or quietly broken (`docs/design/hr.md` § The
  // EU AI Act posture). **Nothing here states a rule the server owns**: which
  // stages exist, whether a record is past its retention date and whether a
  // round still takes applications are all read from the API — these strings
  // only name them. And **a candidate is spoken of as a person**, not as a
  // record moving through a funnel: they applied, somebody met them, somebody
  // decided.
  moduleHr: "People",
  hrTabHiring: "Hiring",
  hrTabTemplates: "Letter templates",
  hrTemplatesTitle: "Letter templates",
  hrTemplatesIntro:
    "Write approved wording once, then let HR create a personal draft without retyping it.",
  hrTemplatesLoadFailed: "The letter templates could not be loaded.",
  hrTemplatesEmpty: "No letter templates yet",
  hrTemplatesEmptyBody:
    "Create the wording your company is prepared to send. Nothing is sent from this screen.",
  hrTemplateNew: "New template",
  hrTemplateCreateTitle: "Create a letter template",
  hrTemplateEditTitle: "Edit letter template",
  hrTemplateEditorIntro:
    "Placeholders are filled only when HR creates a draft for a specific colleague.",
  hrTemplateName: "Template name",
  hrTemplateSubject: "Email subject",
  hrTemplateBody: "Letter wording",
  hrTemplateBodyHint:
    "Use the approved placeholders below. Unknown placeholders are refused.",
  hrTemplateInsertField: "Insert a placeholder",
  hrTemplateSave: "Save template",
  hrTemplateSaveFailed: "The letter template was not saved.",
  hrTemplateDelete: "Delete template",
  hrTemplateDeleteTitle: (name: string) => `Delete ${name}?`,
  hrTemplateDeleteBody:
    "Existing draft letters stay unchanged. This template will no longer be available for new letters.",
  hrTemplateDeleteFailed: "The letter template was not deleted.",
  hrTemplateFields: (count: number) =>
    count === 1 ? "1 placeholder" : `${count} placeholders`,
  hrLoadFailed: "That could not be loaded.",
  hrSaveFailed: "That change was not saved.",
  hrClose: "Close",
  hrCancel: "Cancel",
  hrCreate: "Create",
  hrSave: "Save",

  // Openings — a role written down, then run, then ended.
  hrOpening: "Role",
  hrNewOpening: "New role",
  hrEditOpening: "Edit role",
  hrOpeningSubtitle:
    "A role written down. Publishing says the round is running; closing ends it and freezes what the role said.",
  hrPublishOpening: "Publish",
  hrCloseOpening: "Close round",
  hrCloseConfirm: (title: string) =>
    `Close the round for ${title}? The people who applied stay as the record of what happened, and the round cannot be reopened.`,
  hrIncludeClosed: "Include closed rounds",
  hrClosedNotice:
    "This round is closed. Its board still reads, and the people on it can still be moved — but nobody new can be added.",
  hrOpenedOn: (day: string) => `open since ${day}`,
  hrClosedOn: (day: string) => `closed ${day}`,
  hrStatusDraft: "Draft",
  hrStatusOpen: "Open",
  hrStatusClosed: "Closed",
  hrFieldRole: "Role",
  hrFieldTeam: "Team",
  hrFieldLocation: "Location",
  hrLocationHint: "A city, an office, or “remote”.",
  hrFieldEmployment: "Employment",
  hrKindPermanent: "Permanent",
  hrKindFixedTerm: "Fixed term",
  hrKindPartTime: "Part time",
  hrKindApprentice: "Apprenticeship",
  hrKindContractor: "Contractor",
  hrKindIntern: "Internship",
  hrNoOpeningsTitle: "No roles written down yet",
  hrNoOpeningsBody:
    "Write down the role you are hiring for. Record the people who apply as they come in, and move them along the board as you meet them.",

  // The board — the seven stages the store serves, and a candidate's card.
  hrStage: "Stage",
  hrStageApplied: "Applied",
  hrStageReviewing: "Reviewing",
  hrStageInterview: "Interview",
  hrStageOffer: "Offer",
  hrStageHired: "Hired",
  hrStageRejected: "Not taken further",
  hrStageWithdrawn: "Withdrew",

  // Candidates.
  hrCandidate: "Candidate",
  hrAddCandidate: "Add a candidate",
  hrEditCandidate: "Edit details",
  hrCandidateSubtitle:
    "What the application said. Nothing here is read by a machine — no screening, no ranking, no score.",
  hrFieldName: "Name",
  hrFieldEmail: "Email",
  hrFieldPhone: "Phone",
  hrFieldSource: "Where they came from",
  hrSourceHint:
    "A job board, a referral, an agency — however the application reached you.",
  hrAppliedOn: (moment: string) => `Applied ${moment}`,
  hrNotes: "Interview notes",
  hrNotesEmpty: "Nothing written down yet.",
  hrNotePlaceholder: "What was said in the room…",
  hrAddNote: "Add note",
  hrCv: "CV",
  hrCvNone: "No CV on file.",
  hrCvDownload: "Download the CV",
  hrCvTrashed: "The CV that was on file has been moved to the HR trash.",
  hrCvFailed: "That file could not be downloaded.",
  hrCvAttach: "Attach a CV",
  hrCvHint:
    "Filed in the HR area, where only HR can open it. Nothing reads it — no screening, no ranking, no score.",
  hrCvReplace: "Replace the CV",
  hrCvOnFile: (fileName: string) =>
    fileName === ""
      ? "There is a CV on file. Choosing a file replaces it; the one it replaces goes to the HR trash."
      : `${fileName} is on file. Choosing a file replaces it; the one it replaces goes to the HR trash.`,
  hrCvRemove: "Take the CV off this record",
  hrCvUploadFailed:
    "That file was not uploaded, so nothing was saved. Try again, or save the details without it.",

  // They took the job — the one bridge from the hiring board to the directory.
  hrHired: "They took the job",
  hrHiredExplainer:
    "Moving somebody to Hired records what happened. Writing them into the directory is a separate act, taken here.",
  hrHire: "Add them to the directory",
  hrHireSubmit: "Add to the directory",
  hrHireSubtitle:
    "Their employee record, and the terms they start on. Everything is filled in from the application and the role — correct anything that is not right.",
  hrHireKnown: (name: string) =>
    `${name} is already in the directory with this address. Adding this record would make a second colleague with the same email.`,
  hrHireKnownLeft: (name: string) =>
    `${name} had this address and has left. If this is the same person coming back, adding them here is right — their old record stays as it was.`,
  hrHireNameHint:
    "Split from the name on the application. Correct it if it split wrongly.",
  hrHireEmailHint:
    "Their work address, if it is known yet. It can be added later.",
  hrHireStartHint:
    "The day their terms begin. Every leave balance is counted from it.",
  hrHireNoKind: "Not stated",
  hrHireNoAccount:
    "This writes a record in People. It does not create a login or a mailbox — an administrator does that, and the onboarding checklist has a task for it.",
  hrFieldGivenName: "Given name",
  hrFieldFamilyName: "Family name",
  hrFieldWorkEmail: "Work email",
  hrFieldJobTitle: "Job title",
  hrFieldStartedOn: "Starts on",

  // Retention — a deadline a person acts on, never a job that runs.
  hrRetention: "How long we keep this",
  hrRetentionUntil: (day: string) => `Kept until ${day}.`,
  hrRetentionExpired: "Past its date",
  hrRetentionExplainer:
    "Nothing is erased automatically. When the date has passed, somebody here decides — and what goes, goes: the details, every note, and the CV.",
  hrFieldRetainUntil: "Keep until",
  hrRetainHint:
    "Six months from the application unless you say otherwise. After this date the record can be erased.",
  hrErase: "Erase this record",
  hrEraseConfirm: (name: string) =>
    `Erase everything about ${name}? Their details, every note written about them and their CV are removed for good. This cannot be undone.`,
  // What a website offers, and what it may be ordered from (ADR 0041). Two
  // facts surprise people, so both are said in the screen rather than in a
  // manual: an edit changes the live site only at the next publish, and
  // taking orders is a switch on the catalog, not on the page showing it.
  sitesCatalogs: "Catalog",
  sitesCatalogsHint:
    "What this website offers — dishes, rooms, services, courses. Prices are frozen the moment you publish.",
  sitesCatalogsLoading: "Loading the catalog...",
  sitesCatalogsLoadFailed:
    "The catalogs could not be loaded. Check your connection and try again.",
  sitesCatalogLoadFailed:
    "This catalog could not be opened. Check your connection and try again.",
  sitesNewCatalog: "New catalog",
  sitesCatalogNoneTitle: "Nothing on offer yet",
  sitesCatalogNoneBody:
    "A catalog is the list your website shows — and, if you want, takes orders from. Start with one name and one currency; the items come next.",
  sitesCatalogOrdersOn: "Takes orders",
  sitesCatalogOrdersOff: "No order form",
  sitesCatalogSettings: "This catalog",
  sitesCatalogSettingsHint:
    "The name is yours alone; visitors see the items. Changes reach the live website at your next publish.",
  sitesCatalogName: "Catalog name",
  sitesCatalogCurrency: "Currency",
  sitesCatalogCurrencyHint:
    "Three letters, for example EUR. Changing it re-reads the prices you already wrote in the new currency — it does not convert them.",
  sitesCatalogOrders: "Take orders from this catalog",
  sitesCatalogOrdersHint:
    "Visitors get an order form under the list. Nothing is paid on the website — the order arrives in your inbox and you confirm it yourself. It appears at your next publish.",
  sitesCatalogCreate: "Create catalog",
  sitesCatalogSave: "Save catalog",
  sitesCatalogSaveFailed: "The catalog could not be saved.",
  sitesCatalogDelete: "Delete catalog",
  sitesCatalogDeleteConfirm: "Delete it, with everything in it",
  sitesCatalogDeleteHint:
    "The items and groups go too. Pages already published keep showing what they showed until you publish again.",
  sitesCatalogDeleteFailed: "The catalog could not be deleted.",
  sitesCatalogGroups: "Groups",
  sitesCatalogGroupsHint:
    "Optional. A group is one heading on the page — Breads, Rooms, Half-day courses.",
  sitesCatalogGroupName: "Group name",
  sitesCatalogNewGroup: "New group",
  sitesCatalogNewGroupPlaceholder: "Breads",
  sitesCatalogAddGroup: "Add group",
  sitesCatalogGroupRemove: (name: string) => `Remove the group ${name}`,
  sitesCatalogGroupRemoveShort: "Remove",
  sitesCatalogGroupSaveFailed: "The group could not be saved.",
  sitesCatalogGroupDeleteFailed: "The group could not be removed.",
  sitesCatalogItems: "Items",
  sitesCatalogItemsHint:
    "Everything this catalog offers, in the order the page shows it.",
  sitesCatalogAddItem: "Add an item",
  sitesCatalogNoItemsTitle: "This catalog is empty",
  sitesCatalogNoItemsBody:
    "Add what you offer. A name is enough to start — a price, a photo and a description can follow.",
  sitesCatalogNoPrice: "Price on request",
  sitesCatalogEdit: "Edit",
  sitesCatalogEditItem: (name: string) => `Edit ${name}`,
  sitesCatalogNewItem: "New item",
  sitesCatalogSaveItem: "Save item",
  sitesCatalogItemSubtitle: "It appears on the website at your next publish.",
  sitesCatalogItemName: "Name",
  sitesCatalogItemHandle: "Handle",
  sitesCatalogItemHandlePlaceholder: "From the name",
  sitesCatalogItemHandleHint:
    "The short name used in links and on orders. Leave it empty and we make one from the name.",
  sitesCatalogItemPrice: (currency: string) => `Price (${currency})`,
  sitesCatalogItemPriceHint:
    "Write it as you would on a menu — 4.50 or 4,50. Leave it empty for price on request.",
  sitesCatalogItemPriceNote: "Beside the price",
  sitesCatalogItemPriceNoteHint:
    "A short qualifier — per night, from, per person.",
  sitesCatalogItemGroup: "Group",
  sitesCatalogItemNoGroup: "No group",
  sitesCatalogItemDescription: "Description",
  // The item's photograph. It goes through Drive like every other picture in
  // Sites, so it stays a file the owner can find again; the card on the
  // published page shows it, and what it shows is said in words for anyone
  // who cannot see it.
  sitesCatalogItemPhoto: "Photo",
  sitesCatalogItemPhotoNone: "No photo yet",
  sitesCatalogItemPhotoNoneHint:
    "An item without a photo still appears, with its name, price and description.",
  sitesCatalogItemPhotoAdd: "Add a photo",
  sitesCatalogItemPhotoReplace: "Replace",
  sitesCatalogItemPhotoRemove: "Remove the photo",
  sitesCatalogItemPhotoPreview: "The photo of this item",
  sitesCatalogItemPhotoAlt: "What the photo shows",
  sitesCatalogItemPhotoAltHint:
    "Read aloud by screen readers. Describe the picture — not the name printed under it.",
  sitesCatalogItemPhotoAltMissing:
    "Nobody has described this photo yet; until then the card falls back to the item name.",
  sitesCatalogItemAvailability: "Availability",
  sitesCatalogAvailabilityHint:
    "Sold out still appears, marked and not orderable. Hidden is not published at all.",
  sitesCatalogAvailable: "Available",
  sitesCatalogSoldOut: "Sold out",
  sitesCatalogHidden: "Hidden",
  sitesCatalogItemSaveFailed: "The item could not be saved.",
  sitesCatalogItemDelete: "Delete",
  sitesCatalogItemDeleteConfirm: "Delete it",
  // A list of twenty rows must not offer twenty buttons all called "Delete":
  // the accessible name says which item, and contains the visible word.
  sitesCatalogItemDeleteLabel: (name: string) => `Delete ${name}`,
  sitesCatalogItemDeleteConfirmLabel: (name: string) => `Delete it: ${name}`,
  sitesCatalogItemDeleteFailed: "The item could not be deleted.",
  // Showing a catalog on a page. The section holds a choice, never a copy:
  // which catalog, and optionally which one of its groups. Everything else —
  // the names, the prices, the pictures — is the catalog's own and is frozen
  // into the next publish.
  sitesSectionCatalog: "Catalog",
  sitesSectionCatalogDesc: "What you offer, with prices, from your catalog.",
  sitesCatalogSectionHeading: "Heading above it",
  sitesCatalogSectionChoose: "Which catalog",
  sitesCatalogSectionGroup: "Which group",
  sitesCatalogSectionAllGroups: "Everything in the catalog",
  sitesCatalogSectionGroupHint:
    "Show one group on this page — the lunch menu, the double rooms — or everything.",
  sitesCatalogSectionGoneGroup: (handle: string) =>
    `${handle} (no longer a group)`,
  sitesCatalogSectionOneGroup: (handle: string) => `One group: ${handle}`,
  sitesCatalogSectionNoCatalogs: "This site has no catalog yet",
  sitesCatalogSectionNoCatalogsHint:
    "A catalog holds what you offer, with its prices. Make one and this section can show it.",
  sitesCatalogSectionOrdersOn:
    "This catalog takes orders, so the published page carries an order form under the list. Orders arrive in this site's order inbox.",
  sitesCatalogSectionOrdersOff:
    "This catalog does not take orders, so the page shows the list alone. Ordering is a switch on the catalog, not on this section.",
  // What a visitor may book, and the calendar it is booked into (S2.13c). Two
  // facts are repeated wherever they matter, because a visitor would otherwise
  // discover them: the appointments live in a real Agenda calendar and are
  // managed there, and a free calendar is not an invitation — the opening hours
  // are what is offered, and the calendar only ever takes times away.
  sitesBookings: "Bookings",
  sitesBookingsHint:
    "What visitors can book on this website — a consultation, a viewing, a table. Each one is booked straight into one of your calendars.",
  sitesBookingsLoading: "Loading what can be booked...",
  sitesBookingsLoadFailed:
    "The bookable services could not be loaded. Check your connection and try again.",
  sitesNewBooking: "New bookable service",
  sitesBookingNoneTitle: "Nothing can be booked yet",
  sitesBookingNoneBody:
    "A bookable service is one thing a visitor can take a time for. Say how long it lasts and when you are open for it; the free times are worked out from your calendar.",
  sitesBookingNoCalendarTitle: "No calendar to book into",
  sitesBookingNoCalendarBody:
    "A booking is an appointment in one of your calendars, so there has to be a calendar you can add appointments to. Make one in Agenda and it appears here.",
  sitesBookingSettings: "This service",
  sitesBookingSettingsHint:
    "Everything a visitor is offered. Changes reach the live website at your next publish.",
  sitesBookingName: "What is being booked",
  sitesBookingDescription: "Description",
  sitesBookingWhere: "Where it happens",
  sitesBookingWherePlaceholder: "Second floor, ring the bell",
  sitesBookingWhereLine: (place: string) => `Where: ${place}`,
  sitesBookingCalendar: "Booked into",
  sitesBookingCalendarHint:
    "Appointments are written into this calendar, and times you are already busy there are never offered.",
  sitesBookingCalendarReadOnly: (name: string) =>
    `${name} — shared with you for reading only`,
  sitesBookingCalendarGone: "Calendar no longer available",
  sitesBookingCalendarGoneHint:
    "The calendar this service was booked into can no longer be reached — it was deleted, or its sharing was withdrawn. Until you choose another one, the published page offers no times at all.",
  sitesBookingOpenAgenda: "Open Agenda to manage the appointments",
  sitesBookingLength: "Length (minutes)",
  sitesBookingBuffer: "Gap after (minutes)",
  sitesBookingNotice: "Shortest notice (minutes)",
  sitesBookingHorizon: "Opens ahead (days)",
  sitesBookingTimeZone: "Time zone",
  sitesBookingTimeZoneHint:
    "The clock your opening hours are written on, as an IANA name such as Europe/Brussels. Appointments move with the clock when daylight saving changes.",
  sitesBookingHours: "When you are open for it",
  sitesBookingHoursHint:
    "An empty calendar is not an open day. These windows are what is offered; anything already in the calendar is then taken away.",
  sitesBookingDay: "Day",
  sitesBookingFrom: "From",
  sitesBookingUntil: "Until",
  sitesBookingAddWindow: "Add a window",
  sitesBookingRemoveWindow: (window: string) => `Remove ${window}`,
  sitesBookingNoHours: "No opening hours yet — nothing can be booked.",
  sitesBookingQuestions: "What you ask when it is booked",
  sitesBookingQuestionsHint:
    "A name and an email address are always asked and are not in this list. Add only what this particular booking needs.",
  sitesBookingQuestionLabel: "Question",
  sitesBookingQuestionLabelPlaceholder: "Telephone number",
  sitesBookingQuestionKey: "Stored as",
  sitesBookingQuestionKind: "Kind of answer",
  sitesBookingQuestionText: "One line",
  sitesBookingQuestionLongText: "Several lines",
  sitesBookingQuestionPhone: "Telephone number",
  sitesBookingQuestionChoice: "One of a list",
  sitesBookingQuestionOptions: "The answers offered",
  sitesBookingQuestionOptionsPlaceholder: "Cut, colour, both",
  sitesBookingQuestionRequired: "Must be answered",
  sitesBookingAddQuestion: "Add a question",
  sitesBookingRemoveQuestion: (question: string) =>
    `Remove the question ${question}`,
  sitesBookingActive: "Take bookings for this",
  sitesBookingActiveHint:
    "Switched off, the service stays exactly as it is and the published page says it takes no bookings for now.",
  sitesBookingCreate: "Create service",
  sitesBookingSave: "Save service",
  sitesBookingSaveFailed: "The bookable service could not be saved.",
  sitesBookingDelete: "Delete service",
  sitesBookingDeleteConfirm: "Delete it",
  sitesBookingDeleteHint:
    "Appointments already in your calendar stay exactly as they are — nothing here cancels one. Pages already published keep offering it until you publish again.",
  sitesBookingDeleteFailed: "The bookable service could not be deleted.",
  sitesBookingMinutes: (minutes: number) => `${minutes} minutes`,
  sitesBookingOff: "Not taking bookings",
  sitesBookingPreview: "What a visitor sees",
  sitesBookingPreviewHint:
    "The offer as the published page states it. The free times themselves are worked out against your calendar the moment somebody asks.",
  sitesBookingUnnamed: "Untitled service",
  sitesBookingAsksNothingExtra:
    "Visitors are asked their name and email address.",
  sitesBookingAsksAlso: (questions: string) =>
    `Visitors are asked their name and email address, and: ${questions}.`,
  sitesBookingPublishHint:
    "It appears on the website once a page carries a booking section for it and you publish.",
  sitesBookingOffPreview:
    "This service is switched off, so the page will say it takes no bookings for now.",
  // Offering a booking on a page. The section names a service and nothing else:
  // the length, the week and the questions belong to the service.
  sitesSectionBooking: "Booking",
  sitesSectionBookingDesc:
    "Let visitors book a time with you, straight into your calendar.",
  sitesBookingSectionHeading: "Heading above it",
  sitesBookingSectionChoose: "What can be booked here",
  sitesBookingSectionNoServices: "This site has nothing to book yet",
  sitesBookingSectionNoServicesHint:
    "A bookable service says how long it takes, when you are open for it, and which calendar it goes into. Make one and this section can offer it.",
  sitesBookingSectionOffOption: (name: string) =>
    `${name} (not taking bookings)`,
  sitesBookingSectionLength: (minutes: number) =>
    `Visitors pick a free time of ${minutes} minutes. The times come from your calendar when they ask, not from this page.`,
  sitesBookingSectionOff:
    "This service is switched off, so the published page will say it takes no bookings for now.",
  sitesBookingSectionGone:
    "The service this section offered is gone. Choose another one, or the next publish will be refused.",
  // The ticket shop (ADR 0041): dated events selling seats to an item on the
  // price list. Two facts repeat wherever they matter: nothing here is a
  // second copy (names and prices are Billing's answer at every read), and
  // sold seats are a record — capacity can only shrink to what is left.
  sitesSectionTickets: "Tickets",
  sitesSectionTicketsDesc:
    "The door to your ticket shop. What is on sale, prices and seats stay live.",
  sitesTicketSectionHeading: "Heading above it",
  sitesTicketSectionBody: "Your own words above the link",
  sitesTicketSectionNoEvents: "Nothing is on sale yet",
  sitesTicketSectionNoEventsHint:
    "The published section links to your ticket shop. Create an event so there is something to buy.",
  sitesTicketSectionHint:
    "The published section links to your ticket shop; events, prices and seats are read live when a visitor arrives.",
  sitesTicketSectionOnSale: (count: number) =>
    count === 1 ? "1 event is on sale." : `${count} events are on sale.`,
  sitesTickets: "Tickets",
  sitesTicketsLoadFailed:
    "The events could not be loaded. Check your connection and try again.",
  sitesNoTicketEventsTitle: "No events yet",
  sitesNoTicketEventsBody:
    "A ticketed event sells seats to an item on your price list, on a date. The shop, the checkout and the seat counting are already built — create the first event and your site can sell it.",
  sitesTicketNoProducts: "Nothing is on your price list yet",
  sitesTicketNoProductsHint:
    "An event sells seats to an item from Billing's price list, at that item's own price. Add the item there first; its name and price stay Billing's and are never copied here.",
  sitesNewTicketEvent: "New event",
  sitesNewTicketEventSubtitle:
    "A date, what a seat is sold as, and how many seats there are.",
  sitesTicketCreateSubmit: "Create event",
  sitesTicketCreateFailed: "The event could not be created.",
  sitesTicketEventProduct: "What a seat is sold as",
  sitesTicketEventProductHint:
    "An item from your price list. Its name and price are read live, never copied.",
  sitesTicketProductOption: (name: string, price: string) =>
    `${name} — ${price}`,
  sitesTicketEventStartsAt: "When it starts",
  sitesTicketEventCapacity: "Seats",
  sitesTicketEventCapacityHint:
    "Growing is always allowed. Shrinking stops at the seats already sold or on hold.",
  sitesTicketCapacityTitle: "Change the seats",
  sitesTicketCapacitySubtitle: (taken: number) =>
    taken === 1
      ? "1 seat is already sold or on hold."
      : `${taken} seats are already sold or on hold.`,
  sitesTicketCapacitySubmit: "Save seats",
  sitesTicketCapacityFailed: "The capacity could not be changed.",
  sitesTicketChangeCapacity: "Seats...",
  sitesTicketDelete: "Delete",
  // Named per event (S2.16b2 rule): two rows are otherwise four buttons
  // called "Seats..." and "Delete" with nothing to say which event.
  sitesTicketChangeCapacityFor: (event: string) => `Change seats for ${event}`,
  sitesTicketDeleteFor: (event: string) => `Delete ${event}`,
  sitesTicketDeleteConfirm: "Really delete?",
  sitesTicketDeleteHint:
    "An event nobody has bought into disappears. Once a seat is sold the event is a record of the sale and stays.",
  sitesTicketDeleteFailed: "The event could not be deleted.",
  sitesTicketWhen: "When",
  sitesTicketWhat: "What",
  sitesTicketPrice: "Price",
  sitesTicketSeats: "Seats",
  sitesTicketSeatsCell: (sold: number, remaining: number, capacity: number) =>
    `${sold} sold · ${remaining} of ${capacity} left`,
  sitesTicketHeld: (held: number) =>
    held === 1 ? "(1 in a checkout)" : `(${held} in checkouts)`,
  sitesTicketGoneProduct: "No longer on the price list",
  sitesAssistantSuggestedTickets: "Can I buy tickets online?",
  // The shop shelf (S3.05c): which stocked products a site sells, sharing
  // wave one's checkout. Nothing here is a second copy — names, prices and
  // shelf counts are Billing's and Inventory's answers at every read.
  sitesSectionShop: "Shop",
  sitesSectionShopDesc:
    "The door to your shop. What is on sale, prices and stock stay live.",
  sitesShopSectionHeading: "Heading above it",
  sitesShopSectionBody: "Your own words above the link",
  sitesShopSectionNoItems: "Nothing is in the shop yet",
  sitesShopSectionNoItemsHint:
    "The block links to your shop page. List a stocked product on the Shop screen and it appears there.",
  sitesShopSectionHint:
    "The block links to your shop page. What is on sale, prices and stock are read live — nothing is stored in the page.",
  sitesShopSectionListed: (count: number) =>
    count === 1
      ? "1 product is in the shop."
      : `${count} products are in the shop.`,
  sitesAssistantSuggestedShop: "What do you sell?",
  sitesShop: "Shop",
  sitesShopLoadFailed:
    "The shop could not be loaded. Check your connection and try again.",
  sitesShopAddProduct: "Add product",
  sitesShopAddSubtitle:
    "Pick a stocked product from your price list. Its name, price and stock stay Billing's and Inventory's — the shop only lists it.",
  sitesShopAddSubmit: "Add to shop",
  sitesShopAddFailed: "The product could not be added.",
  sitesShopProduct: "What to sell",
  sitesShopProductHint:
    "Only stocked products from your price list can be sold from the shelf.",
  sitesShopProductOption: (name: string, price: string, units: number) =>
    units === 1
      ? `${name} — ${price} (1 on the shelf)`
      : `${name} — ${price} (${units} on the shelf)`,
  sitesShopColWhat: "What",
  sitesShopColPrice: "Price",
  sitesShopColShelf: "On the shelf",
  sitesShopGoneProduct: "No longer on the price list",
  sitesShopNotStocked: "No longer stocked",
  sitesShopUnits: (units: number) =>
    units === 1 ? "1 unit" : `${units} units`,
  sitesShopRemove: "Remove",
  // Named per product (S2.16b2 rule): a shelf of rows is otherwise a column
  // of buttons all called "Remove" with nothing to say which product.
  sitesShopRemoveFor: (product: string) => `Remove ${product} from the shop`,
  sitesShopRemoveConfirm: "Really remove?",
  sitesShopRemoveHint:
    "Removing only takes the product out of the shop window. Orders already placed keep it.",
  sitesShopRemoveFailed: "The product could not be removed.",
  sitesShopNoProducts: "Nothing is stocked to sell yet",
  sitesShopNoProductsHint:
    "The shop sells stocked products from your price list. Add one in Billing (or let shop setup propose a list), receive stock, and it appears here.",
  sitesShopEmptyTitle: "Your shop window is empty",
  sitesShopEmptyBody:
    "List a stocked product and visitors can buy it on your site, paid on the payment provider's page.",
  sitesShopAllListed: "Every stocked product is already in the shop.",
  sitesShopDeliveryRate: (price: string) =>
    `Delivery is charged at ${price} per order.`,
  sitesShopDeliveryFree: "Delivery is free.",
  sitesCommerceReadOnly:
    "Only this website's owner can change what it sells and charges — you can look, not change.",
  sitesShopDeliveryChange: "Change delivery…",
  sitesShopDeliveryTitle: "Delivery per order",
  sitesShopDeliverySubtitle:
    "One flat rate per order, charged beside the goods. VAT follows the goods.",
  sitesShopDeliveryLabel: (currency: string) => `Delivery price (${currency})`,
  sitesShopDeliveryHint: "0 means delivery is free.",
  sitesShopDeliverySave: "Save delivery",
  sitesShopDeliveryFailed: "The delivery price could not be saved.",
  // Shop setup: the proposed configuration as an approval list (S3.05b3).
  // Every guess is flagged; nothing exists until the owner approves it.
  sitesShopSetup: "Shop setup",
  sitesShopSetupSubtitle:
    "Describe your business and get a proposed price list, VAT treatment and delivery rate to review. Nothing is created until you approve it.",
  sitesShopSetupLoadFailed:
    "The shop setup screen could not be loaded. Check your connection and try again.",
  sitesShopSetupDescribeLabel: "What do you sell?",
  sitesShopSetupDescribeHint:
    "Name what you sell and the prices you charge. Stated prices are used as written — everything else stays a blank or a flagged guess for you to confirm.",
  sitesShopSetupPropose: "Propose a setup",
  sitesShopSetupProposeFailed: "A setup could not be proposed. Try again.",
  sitesShopSetupUnconfigured:
    "This workspace has no AI provider configured, so nothing can be proposed here — set up your price list by hand instead.",
  sitesShopSetupManualPath: "Prefer to do it by hand?",
  sitesShopSetupManualTickets: "Manage ticket events",
  sitesShopSetupManualCatalogs: "Manage catalogs",
  sitesShopSetupExisting: (count: number) =>
    count === 1
      ? "Your price list already has 1 item. Approving adds to it — it never replaces anything."
      : `Your price list already has ${count} items. Approving adds to it — it never replaces anything.`,
  sitesShopSetupProposalTitle: "The proposal",
  sitesShopSetupProposalIntro:
    "Review every row before approving. Prices shown were stated in your description; blanks are yours to fill in, and every VAT rate is a guess to confirm.",
  sitesShopSetupInclude: (name: string) => `Create "${name}"`,
  sitesShopSetupItemName: "Name",
  sitesShopSetupItemUnit: "Unit",
  sitesShopSetupItemPrice: (currency: string) => `Price (${currency})`,
  sitesShopSetupVatLabel: "VAT %",
  sitesShopSetupVatGuessBadge: "VAT is a guess",
  sitesShopSetupNameMissing:
    "Every included item needs a name before approving.",
  sitesShopSetupPriceMissing:
    "Your description didn't state a price — enter one before approving.",
  sitesShopSetupVatMissing:
    "Enter a VAT percentage for every included item before approving.",
  sitesShopSetupKindStock: "Goods",
  sitesShopSetupKindDated: "Tickets",
  sitesShopSetupKindService: "Service",
  sitesShopSetupShippingTitle: "Delivery",
  sitesShopSetupShippingNotNeeded:
    "Nothing in this proposal ships, so there is no delivery rate to set.",
  sitesShopSetupShippingLabel: (currency: string) =>
    `Flat delivery price per order (${currency})`,
  sitesShopSetupShippingMissing:
    "Goods ship, but your description didn't state a delivery rate — enter one before approving.",
  sitesShopSetupShippingCurrent: (price: string) => `Currently ${price}.`,
  sitesShopSetupShippingSaved: "Delivery price saved.",
  sitesShopSetupShippingFailed: "The delivery price could not be saved.",
  sitesShopSetupNothingIncluded:
    "Nothing is ticked — tick at least one item to create it.",
  sitesShopSetupApprove: (count: number) =>
    count === 1 ? "Approve — create 1 item" : `Approve — create ${count} items`,
  sitesShopSetupRetry: "Try again",
  sitesShopSetupDiscard: "Discard proposal",
  sitesShopSetupCreated: "Created",
  sitesShopSetupCreateFailed: "This item could not be created.",
  sitesShopSetupDone: (count: number) =>
    count === 1
      ? "1 item is now on your price list."
      : `${count} items are now on your price list.`,
  sitesShopSetupNextTickets: "Schedule the events tickets are sold for",
  // The order inbox: what visitors asked to buy, and what the owner does next.
  sitesOrders: "Orders",
  sitesOrdersLoadFailed:
    "The orders could not be loaded. Check your connection and try again.",
  sitesOrdersExport: "Export as CSV",
  sitesOrdersExporting: "Exporting...",
  sitesOrdersExportFailed: "The orders could not be exported.",
  sitesNoOrdersTitle: "No orders yet",
  sitesNoOrdersBody:
    "When a published page shows a catalog that takes orders, what visitors ask for lands here — with what they want, their details and the total.",
  sitesOrderList: "Orders",
  sitesOrderDetail: "This order",
  sitesOrderFilter: "Show",
  sitesOrderFilterAll: "All",
  sitesOrderFilterOption: (label: string, count: number) =>
    `${label} (${count})`,
  sitesOrderFilterEmpty: "No orders in this state.",
  sitesOrderStatus: "Where this order stands",
  sitesOrderStatusNew: "New",
  sitesOrderStatusConfirmed: "Confirmed",
  sitesOrderStatusFulfilled: "Done",
  sitesOrderStatusCancelled: "Cancelled",
  sitesOrderStatusFailed: "The order could not be moved.",
  sitesOrderCatalog: "From",
  sitesOrderPhone: "Phone",
  sitesOrderItem: "Item",
  sitesOrderQuantity: "How many",
  sitesOrderUnitPrice: "Each",
  sitesOrderLineTotal: "Line",
  sitesOrderTotal: "Total",
  sitesOrderLinesCaption: "What was ordered",
  sitesOrderLineNoPrice: "On request",
  sitesOrderQuotedHint:
    "An item with no price adds nothing to the total — quote it yourself when you reply.",
  sitesOrderLineCount: (count: number) =>
    count === 1 ? "1 item" : `${count} items`,
  sitesOrderDelete: "Delete order",
  sitesOrderDeleteConfirm: "Delete it for good",
  sitesOrderDeleteHint:
    "This order holds someone's name, phone number and what they asked for. Deleting removes all of it — there is no undo.",
  sitesOrderDeleteFailed: "The order could not be deleted.",
  sitesCollections: "Collections",
  sitesCollectionsHint:
    "Turn an alo Base table into reusable cards for your website.",
  sitesConnectTable: "Connect a table",
  sitesCollectionsLoading: "Loading collections...",
  sitesCollectionsLoadFailed:
    "Collections could not be loaded. Check your connection and try again.",
  sitesCollectionEmptyTitle: "Connect your first table",
  sitesCollectionEmptyBody:
    "Choose an alo Base, match its columns once, and reuse those rows on any page.",
  sitesCollectionNoBasesTitle: "Create an alo Base first",
  sitesCollectionNoBasesBody:
    "Collections read rows from alo Base. Create a Base in Drive, then return here to connect it.",
  sitesCollectionOpenDrive: "Open Drive",
  sitesCollectionName: "Collection name",
  sitesCollectionBase: "alo Base",
  sitesCollectionTable: "Table",
  sitesCollectionChooseBase: "Choose a Base",
  sitesCollectionChooseTable: "Choose a table",
  sitesCollectionRows: (count: number) =>
    count === 1 ? "1 row" : `${count} rows`,
  sitesCollectionConnectedTo: (base: string, table: string) =>
    `${base} / ${table}`,
  sitesCollectionSourceUnavailable:
    "Choose the Base and table whose rows should appear on the website.",
  sitesCollectionEdit: (name: string) => `Edit ${name}`,
  sitesCollectionMapping: "Match columns to website content",
  sitesCollectionMappingHint:
    "The title is required. Everything else is optional and can be added later.",
  sitesCollectionOptional: "Optional",
  sitesCollectionNotMapped: "Do not show",
  sitesCollectionNoCompatibleField: "This table needs a text column",
  sitesCollectionTitleField: "Title",
  sitesCollectionSlugField: "Page path",
  sitesCollectionSummaryField: "Summary",
  sitesCollectionBodyField: "Body",
  sitesCollectionImageField: "Image",
  sitesCollectionLinkField: "Link",
  sitesCollectionDateField: "Published date",
  sitesCollectionSave: "Save collection",
  sitesCollectionSaving: "Saving...",
  sitesCollectionSaveFailed:
    "The collection was not saved. Nothing changed; check the highlighted mapping and try again.",
  sitesCollectionDisconnect: "Disconnect",
  sitesCollectionDisconnectConfirm: "Disconnect now",
  sitesCollectionDisconnectHint: "The Base and all its rows stay in Drive.",
  sitesCollectionDisconnectFailed:
    "The collection is still connected. Remove it from any pages that use it, then try again.",
  sitesCollectionPreview: "Current rows",
  sitesCollectionPreviewHint:
    "This is exactly what the next publish will read from Base.",
  sitesCollectionPreviewLoading: "Loading the current Base rows",
  sitesCollectionPreviewFailed:
    "These rows could not be previewed. Fix the Base value named by the server, then try again.",
  sitesCollectionPreviewSaveTitle: "Save to preview these rows",
  sitesCollectionPreviewSaveBody:
    "Once connected, the same publish rules used by the live site will check every row here.",
  sitesCollectionPreviewEmptyTitle: "This table has no complete rows yet",
  sitesCollectionPreviewEmptyBody:
    "Add a title to a row in Base and it will appear here automatically.",
  sitesCollectionPreviewLinked: "Opens a link",
  sitesSectionCollection: "Collection",
  sitesSectionCollectionDesc: "A reusable grid of rows from alo Base.",
  sitesCollectionSectionHeading: "Section heading",
  sitesCollectionSectionChoose: "Collection to show",
  sitesCollectionSectionNoConnections:
    "Connect a table before adding this section",
  sitesCollectionSectionNoConnectionsHint:
    "The collection stays reusable, so the same Base can power more than one page.",

  // The sandboxed custom-code block (S2.14b). The words do the work the CSP
  // does: they say what the frame stops before the first line is written, so
  // nobody pastes an embed and discovers at publish time that it cannot load.
  sitesSectionCustomCode: "Custom code",
  sitesSectionCustomCodeDesc:
    "Your own HTML, CSS and JavaScript, sealed in a frame with no way out.",
  sitesCustomCodeBoundaryTitle: "What this block can and cannot do",
  sitesCustomCodeBoundarySealed:
    "It runs sealed off from your site: it cannot read the page around it, your visitors, or anything they typed elsewhere.",
  sitesCustomCodeBoundaryNoNetwork:
    "It has no network. Nothing loads from another address — no embed, no font, no analytics script — which is what keeps this site free of a cookie banner.",
  sitesCustomCodeBoundaryYours:
    "It is your code, published exactly as you wrote it. We do not check what it does, and the assistant will not write or change it.",
  sitesCustomCodeHeadingHint:
    "Shown by the page above the block, in your site's own type. Leave it empty for a block that stands alone.",
  sitesCustomCodeFrameTitle: "What this block is",
  sitesCustomCodeFrameTitleHint:
    'Read aloud to visitors using a screen reader — "A timer counting down the current roast", not "frame".',
  sitesCustomCodeHtml: "Markup",
  sitesCustomCodeHtmlHint:
    "The body of the block. The document around it — its policy, its style and script blocks — is written for you.",
  sitesCustomCodeCss: "Style",
  sitesCustomCodeCssHint: "Applies inside this block only. Optional.",
  sitesCustomCodeJs: "Script",
  sitesCustomCodeJsHint:
    "Runs inside this block only, on the visitor's device.",
  sitesCustomCodeCapabilities: "What the block is allowed to do",
  sitesCustomCodeCapabilitiesHint:
    "Everything is off until you turn it on, and only these two can be turned on.",
  sitesCustomCodeScripts: "Run a script",
  sitesCustomCodeScriptsHint:
    "Without this the block is markup and style: nothing in it executes, whatever it says.",
  sitesCustomCodeScriptMissing:
    "There is no script to run yet. Write one, or switch this off — a permission with nothing behind it is refused.",
  sitesCustomCodeScriptDropped:
    "Switched off, so the script below is not saved with the block. Switch it back on to keep it.",
  sitesCustomCodeImages: "Show images carried in the markup",
  sitesCustomCodeImagesHint:
    "For a picture written into the markup itself. A picture from an address still cannot load — use an image section for those.",
  sitesCustomCodeHeight: "Height on the page (pixels)",
  sitesCustomCodeHeightHint:
    "A sealed block cannot be measured from the outside, so you say how tall it is. Between 40 and 2000.",
  sitesCustomCodeBytes: (used: number, max: number) =>
    `${used} of ${max} bytes`,
  sitesCustomCodeBytesOver: (used: number, max: number) =>
    `${used} of ${max} bytes — too long to save`,
  sitesCustomCodeTotalBytes: (used: number, max: number) =>
    `${used} of ${max} bytes in this block altogether`,

  // The addresses a website answers to (S2.15c3): the alo one it always has,
  // a domain its owner already owns, and one bought here. Two rules run
  // through this copy. Every price is said twice — what it costs today and
  // what it costs every year afterwards — because the renewal is the half a
  // bait price hides in. And nothing is called finished before it is: a TXT
  // record that has not travelled yet is "not found yet", an approval is not a
  // payment, and a payment is not a registration.
  sitesDomains: "Domains",
  sitesDomainsLoading: "Loading the domains...",
  sitesDomainsLoadFailed:
    "The domains of this website could not be loaded. Check your connection and try again.",
  sitesDomainAloAddress: "This website is always reachable at",
  sitesDomainOwned: "A domain you already own",
  sitesDomainOwnedHint:
    "Add the domain, publish the record shown at your DNS host, then press Check. Nothing changes for your visitors until it is verified.",
  sitesDomainAddress: "Domain",
  sitesDomainPlaceholder: "example.com",
  sitesDomainAdd: "Add domain",
  sitesDomainAddFailed: "That domain could not be added.",
  sitesDomainNoneBody:
    "No domain of your own is connected yet. Add one you already own, or buy one below, and this website answers there too.",
  sitesDomainStatusPending: "Waiting for the record",
  sitesDomainStatusVerified: "Verified",
  sitesDomainStatusLive: "Serving",
  sitesDomainCheck: "Check",
  sitesDomainVerifyFailed: "The domain could not be checked.",
  sitesDomainNotYet:
    "The record is not visible yet. DNS changes take a few minutes to travel — leave the record in place and check again shortly.",
  sitesDomainVerifiedNow: (domain: string) =>
    `${domain} is verified. This website now answers there.`,
  sitesDomainRecordTitle: "Publish this record at your DNS host",
  sitesDomainRecordName: "Name",
  sitesDomainRecordType: "Type",
  sitesDomainRecordValue: "Value",
  sitesDomainRecordHint:
    "Keep the record in place until the check succeeds. Some DNS hosts add the domain to the name themselves — if yours does, leave it out.",
  sitesDomainPointHint: (host: string) =>
    `Last step at your DNS host: point the domain at ${host} with a CNAME. An apex domain needs your host's ALIAS or ANAME record instead.`,
  sitesDomainCopy: "Copy",
  sitesDomainCopied: "Copied",
  sitesDomainRemove: "Remove",
  sitesDomainRemoveConfirm: "Yes, remove it",
  sitesDomainRemoveHint:
    "alo stops answering at this domain. The domain itself stays yours — nothing is given up at the registry.",
  sitesDomainRemoveFailed: "That domain could not be removed.",

  sitesDomainBuy: "Buy a domain",
  sitesDomainBuyHint:
    "Search for a name. You see what it costs this year and every year after it before anything is bought.",
  sitesDomainSearchLabel: "The name you would like",
  sitesDomainSearchPlaceholder: "acme",
  sitesDomainSearching: "Looking...",
  sitesDomainSearchInvite: "Type a name to see which endings are free.",
  sitesDomainSearchFailed: "That name could not be checked.",
  sitesDomainCatalogFailed: "The domain prices could not be loaded.",
  sitesDomainUnconfiguredTitle: "Buying domains is not switched on here",
  sitesDomainUnconfiguredBody:
    "This workspace cannot register domain names. You can still connect a domain you already own.",
  sitesDomainNotBuyable:
    "This workspace can show prices but cannot register a domain yet, because it has no nameservers configured.",
  sitesDomainTestRegistrar: (name: string) =>
    `${name} is a test registrar: nothing is charged and no real name is registered.`,
  sitesDomainRegistrarLine: (name: string, country: string) =>
    `Domains are registered through ${name} (${country}). Prices exclude VAT.`,
  sitesDomainAvailable: "Free",
  sitesDomainTaken: "Already registered",
  sitesDomainBlocked: "Not for sale",
  sitesDomainUnsupportedEnding: "alo does not sell this ending",
  sitesDomainPremium: "Premium name",
  sitesDomainPremiumHint:
    "The registry prices this name above its ending's usual price. Its renewal price is the one shown, not the ordinary one.",
  sitesDomainPriceLine: (today: string, renewal: string) =>
    `${today} today, then ${renewal} every year`,
  sitesDomainChoose: "Buy this domain",

  sitesDomainPurchaseTitle: (domain: string) => `Buy ${domain}`,
  sitesDomainPurchaseSubtitle:
    "Who the domain is registered to, and for how long. You approve the price on the next step; nothing is charged before that.",
  sitesDomainYears: "Paid for",
  sitesDomainYearsHint:
    "How many years the first payment covers. After that it is one year at a time.",
  sitesDomainYearsOption: (years: number) =>
    years === 1 ? "1 year" : `${years} years`,
  sitesDomainAutoRenew: "Renew this domain automatically",
  sitesDomainAutoRenewHint:
    "A domain that is not renewed is lost, and anybody may then take it. Switch this off only if you intend to renew it yourself.",
  sitesDomainAutoRenewOn: "It renews automatically every year.",
  sitesDomainAutoRenewOff:
    "It does not renew automatically: you have to renew it yourself before it expires, or you lose it.",
  sitesDomainRegistrant: "Registered to",
  sitesDomainRegistrantHint:
    "The registry requires a real person or company that can be reached. This goes to the registry — it is never shown on your website.",
  sitesDomainRegistrantName: "Full name",
  sitesDomainRegistrantOrganisation: "Company (leave empty if there is none)",
  sitesDomainRegistrantEmail: "Email",
  sitesDomainRegistrantEmailHint:
    "The registry writes here about expiry and verification. An address nobody reads loses the domain.",
  sitesDomainRegistrantStreet: "Street and number",
  sitesDomainRegistrantPostalCode: "Postal code",
  sitesDomainRegistrantCity: "City",
  sitesDomainRegistrantCountry: "Country",
  sitesDomainRegistrantCountryHint:
    "The two-letter country code, such as nl or be.",
  sitesDomainRegistrantPhone: "Telephone",
  sitesDomainRegistrantPhoneHint:
    "In international form, such as +31201234567.",
  sitesDomainRequirementEea:
    "This ending is only sold to a registrant inside the European Economic Area.",
  sitesDomainRequirementCountry: (country: string) =>
    `This ending is only sold to a registrant in ${country}.`,
  sitesDomainSeePrice: "See the price",
  sitesDomainQuoteFailed: "That domain could not be priced.",
  sitesDomainApproveTitle: "Approve this price",
  sitesDomainApproveSubtitle: (domain: string) =>
    `What ${domain} costs, in full, before anything is charged.`,
  sitesDomainQuoteName: "Domain",
  sitesDomainQuoteTerm: "Paid for",
  sitesDomainQuoteToday: "Today",
  sitesDomainQuoteRenewal: "Every year after",
  sitesDomainApproveAction: (price: string) => `Approve ${price}`,
  sitesDomainApproveHint:
    "Approving records that you agreed to these exact amounts. If the price changes before it is paid, alo asks you again instead of charging a different one.",
  sitesDomainApproveFailed: "That price could not be approved.",

  sitesDomainPurchases: "Domains bought here",
  sitesDomainPurchasesHint:
    "Every domain this website has started buying, and where it got to.",
  sitesDomainPurchasesNone: "No domain has been bought for this website yet.",
  sitesDomainPurchasesLoadFailed: "The domain purchases could not be loaded.",
  sitesDomainRefresh: "Refresh",
  sitesDomainTermPrice: (price: string, years: number) =>
    years === 1
      ? `${price} for the first year`
      : `${price} for the first ${years} years`,
  sitesDomainRenewalLine: (price: string) => `then ${price} a year`,
  sitesDomainApprovedOn: (when: string) => `Price approved on ${when}.`,
  sitesDomainAttempts: (attempts: number) =>
    `Registration attempt ${attempts}; alo keeps trying.`,
  sitesDomainCancel: "Call it off",
  sitesDomainCancelConfirm: "Yes, call it off",
  sitesDomainCancelFailed: "That purchase could not be called off.",
  sitesDomainStateQuoted: "Waiting for your approval",
  sitesDomainStateApproved: "Approved",
  sitesDomainStateAwaitingPayment: "Waiting to be paid",
  sitesDomainStatePaid: "Paid",
  sitesDomainStateRegistering: "Registering",
  sitesDomainStateRegistered: "Registered",
  sitesDomainStateConfigured: "Live",
  sitesDomainStateFailed: "Not completed",
  sitesDomainStateCancelled: "Called off",
  sitesDomainStepQuoted:
    "Nothing has been charged. Approve the price and the purchase goes on to payment.",
  sitesDomainStepApproved:
    "You approved this price. Payment comes next: once it settles, alo registers the domain and attaches it to this website by itself.",
  sitesDomainStepAwaitingPayment:
    "Waiting for the payment to settle. Registration starts by itself the moment it does.",
  sitesDomainStepPaid: "Paid. Registration starts within a minute.",
  sitesDomainStepRegistering:
    "The registrar is registering the name right now.",
  sitesDomainStepRegistered: (domain: string) =>
    `${domain} is registered to you. Attaching it to this website.`,
  sitesDomainStepConfigured: (domain: string) =>
    `${domain} is registered and serving this website.`,
  sitesDomainStepFailed:
    "This purchase could not be completed. Nothing further will be charged for it.",
  sitesDomainStepCancelled: "Called off. Nothing was charged.",
  sitesDomainOwnerOnly:
    "Only this website’s owner can buy or manage its domain names. You can still edit and publish the website itself.",

  // The one approvals inbox (B6.07) — leave, expense claims and timesheet weeks
  // in one list. The words are the approver's, not the system's: what is
  // waiting is "time off", "a claim", "a week", and the person who handed it in
  // is waiting for an answer rather than sitting in a queue. "Send back" rather
  // than "reject", because that is what actually happens: the record returns to
  // its owner, editable, with the sentence that says what to fix.
  hrTabApprovals: "Approvals",
  hrQueueLeave: "Time off",
  hrQueueExpense: "Claim",
  hrQueueTimesheet: "Week",
  hrPerson: "Person",
  hrWhat: "Waiting for you",
  hrQueue: "Kind",
  hrFigure: "Amount",
  hrWaitingSince: "Handed in",
  hrActions: "Decision",
  // The names the design system asks for and never draws: what a row of
  // controls acts on (`ds/Toolbar`), and what a table lists (`ds/Table`'s
  // caption). Read out by a screen reader, which is the only place two
  // toolbars on one screen are told apart (D2.08b).
  hrHiringControls: "Hiring round",
  hrLeaveControls: "Leave filters",
  hrAwayControls: "Month",
  hrDirectoryControls: "Directory filters",
  hrLeaveTable: "Leave requests",
  hrApprovalsTable: "Waiting for a decision",
  hrDirectoryTable: "People",
  hrApprove: "Approve",
  hrSendBack: "Send back",
  hrSendBackTitle: "Send this back?",
  hrSendBackBody: (person: string) =>
    `${person} will see this again, editable, with what you write here. Say what needs correcting.`,
  hrSendBackPlaceholder: "What needs correcting",
  hrWaitingCount: (count: number) =>
    count === 1 ? "1 waiting" : `${count} waiting`,
  hrCountOf: (kind: string, count: number) => `${kind}: ${count}`,
  hrWorkingDays: (days: number) => (days === 1 ? "1 day" : `${days} days`),
  hrLeaveOf: (policy: string, from: string, to: string) =>
    from === to ? `${policy}, ${from}` : `${policy}, ${from} – ${to}`,
  hrApprovalsEmptyTitle: "Nothing is waiting",
  hrApprovalsEmptyBody:
    "Time off, expense claims and timesheet weeks people hand in land here together, oldest first — so nobody waits because their request was in the module you opened last.",
  hrApprovalsNoneTitle: "Nothing comes to you for a decision",
  hrApprovalsNoneBody:
    "This is where leave, expense claims and timesheet weeks wait for the person who decides them. You will see it when somebody reports to you, or when you look after the books.",
  hrApprovalsQueueFailed: (kinds: string) =>
    `Some of what is waiting could not be read (${kinds}), so this list is not complete. Everything else is shown.`,
  hrApprovalsWidgetLabel: "waiting",
  hrApprovalsWidgetTitle:
    "Time off, claims and weeks waiting for your decision",

  // The directory and the org chart (B6.08a) — the module's one screen with no
  // door on it, and the words are a colleague's rather than a system's. Nobody
  // is an "employee record" here: they are a person, they are here since a day,
  // and they report to somebody. "Left" is what a person did, said plainly and
  // without a euphemism, because a directory that hides it is a directory that
  // sends mail to somebody who is gone.
  hrTabDirectory: "Directory",
  hrDirectorySearch: "Search people",
  hrDirectoryViews: "How to read the directory",
  hrViewPeople: "People",
  hrViewOrg: "Org chart",
  hrIncludeLeavers: "Include people who have left",
  hrPeopleCount: (count: number) =>
    count === 1 ? "1 person" : `${count} people`,
  hrShowingOf: (shown: number, total: number) => `${shown} of ${total}`,
  hrContact: "Contact",
  hrManager: "Reports to",
  hrSince: "Here since",
  hrYou: "You",
  hrLeft: "Left",
  hrShowInChart: "Where they sit",
  hrReportsCount: (count: number) =>
    count === 1 ? "1 report" : `${count} reports`,
  hrDirectoryEmptyTitle: "Nobody is in the directory yet",
  hrDirectoryEmptyBody:
    "As soon as HR writes down the first person, this is where everybody finds their colleagues — who they are, how to reach them, and who they report to.",
  hrNoMatchTitle: (query: string) => `Nobody matches “${query}”`,
  hrNoMatchBody:
    "Names, roles, teams, email addresses and telephone numbers are all searched, in any order. Try one word less.",
  hrClearSearch: "Clear the search",

  // Time off (B6.08b) — a person's own leave, the decisions on somebody else's,
  // and who is away. Two habits run through the words. First, no figure is
  // written here: every number these strings frame is the server's fold over a
  // working pattern and the tenant's public holidays, and the balance is shown
  // with its working because a balance nobody can reproduce is a balance nobody
  // believes. Second, nothing is called a "leave request" to the person making
  // one — they are asking for time off, and the record is our word for it.
  hrTabLeave: "My leave",
  hrTabAway: "Who’s away",
  hrLeaveWhose: "Whose time off",
  hrScopeMine: "Mine",
  hrScopeTeam: "My team",
  hrScopeEveryone: "Everyone",
  hrLeaveShow: "Show",
  hrShowEverything: "Everything",
  hrShowWaiting: "Waiting for a decision",
  hrShowBooked: "Booked",
  hrAskForLeave: "Ask for time off",
  hrOneDay: "1 day",
  hrDaysOf: (days: string) => `${days} days`,
  hrFactOf: (label: string, value: string) => `${label} ${value}`,
  hrBalanceLeft: "left",
  hrBalanceThisYear: "This year",
  hrBalanceTaken: "Taken",
  hrBalanceBooked: "Booked",
  hrBalanceWaiting: "Waiting",
  hrBalanceAsOf: (day: string) =>
    `Worked out on ${day}, on your own working pattern.`,
  hrUnpaid: "Unpaid",
  hrNotDecided: "Recorded, not decided",
  hrLeaveKind: "Kind",
  hrLeaveWhen: "When",
  hrLeaveDays: "Days",
  hrLeaveWhy: "Why",
  hrLeaveState: "State",
  hrLeaveBetween: (from: string, to: string) => `${from} – ${to}`,
  hrHolidaysInside:
    "A public holiday falls inside these dates and is not counted.",
  hrLeaveRequested: "Waiting",
  hrLeaveApproved: "Booked",
  hrLeaveRejected: "Not agreed",
  hrLeaveWithdrawn: "Taken back",
  hrLeaveCancelled: "Cancelled",
  hrWithdraw: "Take it back",
  hrCancelLeave: "Cancel it",
  hrLeaveEmptyTitle: "You have not asked for any time off",
  hrLeaveEmptyBody:
    "Ask for a day or a fortnight here. You will see what it costs your balance before anybody decides, and who else is already off on those days.",
  hrLeaveTeamEmptyTitle: "Nobody has asked for time off",
  hrLeaveTeamEmptyBody:
    "When somebody who reports to you asks for days off, they arrive here and in your approvals inbox — with the dates, what it costs their balance, and who else is away then.",
  hrLeaveNoneShownTitle: "Nothing in that state",
  hrLeaveNoneShownBody:
    "There is time off recorded, but none of it is in the state you asked for.",
  hrAskSubtitle:
    "The days come off the balance for the kind you pick, worked out from your own working pattern — you never type a number of days.",
  hrAskSubmit: "Ask",
  hrPolicyRecordedHint:
    "This kind is recorded rather than decided: it is booked as soon as you ask.",
  hrFieldFirstDay: "First day off",
  hrFieldLastDay: "Last day off",
  hrLastDayHint: "The day you come back is not part of it.",
  hrRangeBackwards: "The last day is before the first one.",
  hrAlsoAway: "Already off then",
  hrNobodyAway: "Nobody else is off on those days.",
  hrWhyHint:
    "Optional. Only whoever decides this reads it, and it is never logged.",

  // The absence calendar. It says who, and when — and nothing else, because the
  // route behind it does not carry a reason and never should.
  hrAwayCalendar: "Who is away, by day",
  hrPreviousMonth: "The month before",
  hrNextMonth: "The month after",
  hrThisMonth: "This month",
  hrAwayThisMonth: (count: number) =>
    count === 1
      ? "1 person away this month"
      : `${count} people away this month`,
  hrMoreAway: (count: number) => `+${count} more`,
  hrDayAway: (day: string, count: number) =>
    count === 0 ? `${day}: nobody away` : `${day}: ${count} away`,
  hrNobodyAwayTitle: (month: string) => `Nobody is away in ${month}`,
  hrNobodyAwayBody:
    "Booked time off appears here for everybody in the company, so you can see who is out before you plan around them. Public holidays are marked too.",

  // alo Campaigns (ADR 0044, wave C1) — the audience screen. Nothing on it
  // sends: the sending identity waits on a second IP, and a button promising a
  // send would be a promise the product cannot keep. The words that matter most
  // here are the ones naming who will NOT be mailed — a count without them is
  // a number a colleague finds out about by sending.
  moduleCampaigns: "Campaigns",
  campaignsTitle: "Audience",
  campaignsSubtitle:
    "Everyone this workspace could reach, and everyone it may not — with the reason.",
  campaignsCountriesLabel: "Countries",
  campaignsCountriesHint:
    "Two-letter codes, separated by commas. Empty means everywhere.",
  campaignsCountriesPlaceholder: "BE, NL",
  campaignsPurchaseLabel: "Purchases",
  campaignsPurchaseAny: "Anyone",
  campaignsPurchaseBought: "Has bought",
  campaignsPurchaseNotBought: "Has not bought",
  campaignsPeriodLabel: "In the last",
  campaignsPeriodEver: "Ever",
  campaignsPeriodDays: (days: number) => `${days} days`,
  campaignsEveryone: "Everyone",
  campaignsSegmentsLabel: "Saved questions",
  campaignsSaveSegment: "Save this question",
  campaignsSegmentNamePrompt: "What should this question be called?",
  campaignsSegmentNamePlaceholder: "Belgian customers",
  campaignsDeleteSegment: "Delete",
  campaignsDeleteSegmentConfirm: (name: string) =>
    `Delete the question “${name}”? Nobody’s agreement or unsubscribe is touched — only the question goes.`,
  campaignsTallyMailable: (mailable: number, matched: number) =>
    `${mailable} of ${matched} people will be mailed`,
  campaignsTallyNobody: "Nobody in this workspace matches that question.",
  campaignsExcludedCount: (people: number, reason: string) =>
    `${people} · ${reason}`,
  campaignsWillBeMailed: "Will be mailed",
  campaignsReasonNoConsent: "Never agreed",
  campaignsReasonUnsubscribe: "Unsubscribed",
  campaignsReasonHardBounce: "Mail bounced",
  campaignsReasonComplaint: "Reported as spam",
  campaignsReasonManual: "Asked us to stop",
  campaignsTableLabel: "People this question selects",
  campaignsColPerson: "Person",
  campaignsColCountry: "Country",
  campaignsColKnownFrom: "Known from",
  campaignsColStatus: "Status",
  campaignsSourceBillingCustomer: "Customer",
  campaignsSourceCrmDeal: "Deal",
  campaignsSourceSiteForm: "Website form",
  campaignsNoMatches: "Nobody matches that question.",
  campaignsMore: "Show more people",
  campaignsLoadFailed: "The audience could not be read.",
  campaignsSegmentsFailed: "Your saved questions could not be read.",
  campaignsSaveFailed: "That question could not be saved.",
  campaignsDeleteFailed: "That question could not be removed.",
  campaignsEmptyTitle: "Nobody to reach yet",
  campaignsEmptyBody:
    "People appear here as soon as this workspace has a customer, a deal with an email address, or somebody who filled in a form on its website. Personal address books are never used.",
  campaignsNothingSentYet:
    "Nothing is sent from this screen. Campaign sending needs its own address, separate from your everyday mail, so that a newsletter can never affect whether your invoices arrive.",

  // The letter as one person will actually receive it (wave C3.6). The words
  // that matter most here are the caveat and the "show as" labels: a preview
  // is our renderer's opinion, and the copy nobody proof-reads is the one that
  // goes to everybody with no name on file.
  campaignsViewsLabel: "What to look at",
  campaignsTabAudience: "Audience",
  campaignsTabLetters: "Letters",
  campaignsLettersTitle: "Letters",
  campaignsLettersSubtitle:
    "Each letter as one person will actually receive it.",
  campaignsLetterLabel: "Letter",
  campaignsNoLettersTitle: "No letters yet",
  campaignsNoLettersBody:
    "A letter is written in the same editor as a document: headings, paragraphs, tables and code. Once one exists it appears here, rendered exactly as it will arrive.",
  campaignsShowAsLabel: "Show as",
  campaignsShowAsHint: "Both are real. Half an audience has no name on file.",
  campaignsShowAsRecipient: "Someone you can mail",
  campaignsShowAsFallbacks: "Someone with nothing on file",
  campaignsPartLabel: "Part",
  campaignsPartHint:
    "Every letter carries both. Some people, and every filter, read the plain one.",
  campaignsPartHtml: "Formatted",
  campaignsPartText: "Plain text",
  campaignsPreviewFrameLabel: "The letter as it will be received",
  campaignsPreviewSubject: "Subject",
  campaignsPreviewPreheader: "Preview text",
  campaignsPreviewNoPreheader:
    "None — mail clients will show the first line of the letter instead.",
  campaignsAgainstRecipient: (person: string) =>
    `This is the copy ${person} receives.`,
  campaignsAgainstFallbacks:
    "This is the copy everyone with nothing on file receives — every personalised value below is your own fallback wording.",
  campaignsAgainstNobodyYet:
    "There is nobody to mail yet, so this is the copy someone with nothing on file receives. Every personalised value below is your own fallback wording.",
  campaignsPreviewCaveat:
    "This is our renderer’s opinion, not proof. Outlook on Windows draws mail with Word’s engine and every client differs — put a test copy in your Drafts and read it where your recipients will.",
  campaignsTestDraft: "Put a test copy in my Drafts",
  campaignsTestDraftDone: (address: string) =>
    `A copy is in your Drafts, addressed to ${address}. Nothing was sent — open it in your mail app, or send it to yourself to see how a real client draws it.`,
  campaignsTestDraftFailed: "That test copy could not be written.",
  campaignsFieldsTitle: "What the personalised values became",
  campaignsColField: "Value",
  campaignsColPrinted: "Reads as",
  campaignsColWhoseWords: "Whose words",
  campaignsFieldTheirs: "From their record",
  campaignsFieldFallback: "Your fallback",
  campaignsNoFields: "This letter says the same thing to everybody.",
  campaignsFieldFirstName: "First name",
  campaignsFieldName: "Full name",
  campaignsFieldEmail: "Email address",
  campaignsFieldCountry: "Country",
  campaignsVocabularyTitle: "What you can personalise",
  campaignsFieldExample: (field: string) => `{{${field}|your words}}`,
  campaignsVocabularyHint:
    "The words after the bar are what someone with nothing on file reads. They are not optional: a value with no fallback is where “Hi ,” comes from.",
  campaignsLettersFailed: "Your letters could not be read.",
  campaignsPreviewFailed: "That letter could not be rendered.",

  // The page at the end of an unsubscribe link — the only screen in this
  // product a stranger reads, and they arrive at it already annoyed. Every
  // sentence here is plain, short and says exactly what a press did: the
  // alternative to understanding this page is the spam button, and that is the
  // one signal a sending reputation does not survive.
  //
  // Nothing here names the recipient. A link is forwarded, quoted in replies and
  // read by every scanner between the sender and the inbox, so the server never
  // returns the address and no string may interpolate one.
  campaignUnsubscribeLoading: "Checking this link…",
  campaignUnsubscribeTitle: "Stop these emails",
  campaignUnsubscribeSubtitle: (topic: string) =>
    `This message was sent as “${topic}”. You can stop that kind on its own, or stop everything.`,
  campaignUnsubscribeSubtitleUntopiced:
    "You can stop receiving email from this workspace. One press is all it takes.",
  campaignUnsubscribeStopTopic: (topic: string) => `Stop sending me “${topic}”`,
  campaignUnsubscribeStopAll: "Stop sending me anything",
  campaignUnsubscribeAlreadyStopped:
    "This workspace has already been told to stop emailing you. You do not need to do anything else.",
  campaignUnsubscribeAlreadyDeclined: (topic: string) =>
    `You have already stopped “${topic}”. You can still stop everything else below.`,
  campaignUnsubscribeDoneTitle: "Done",
  // The words of the unsubscribe link in a campaign's own footer (C2.5).
  // Sent to the server, which has no translations of its own, and printed in
  // the letter the recipient receives — so it is read by somebody who has
  // already decided they want the mail to stop, and must say exactly that
  // rather than a euphemism like "manage your preferences".
  campaignUnsubscribeLinkText: "Unsubscribe",
  campaignUnsubscribeDoneAll:
    "This workspace will not email you again. Nothing further is needed.",
  campaignUnsubscribeDoneTopic: (topic: string) =>
    `You will not be sent “${topic}” again.`,
  campaignUnsubscribeDoneTopicNote:
    "Other kinds of email from this workspace — invoices and replies, for example — will still reach you. Come back to this link to stop those too.",
  campaignUnsubscribeFinalNote:
    "This cannot be undone from here. If you change your mind, ask the sender directly.",
  campaignUnsubscribeNoAccountNote:
    "No account and no sign-in is needed. This page is only about the address this message was sent to.",
  campaignUnsubscribeUnknownTitle: "This link no longer works",
  campaignUnsubscribeUnknownLink:
    "We do not recognise this unsubscribe link. If you copied it from an email, open the link from the email itself — or reply to the sender and ask them to stop.",
  campaignUnsubscribeFailed:
    "That could not be saved just now. Please press the button again.",
  billingImportPrices: "Import prices",
  billingPriceList: "price list",
  billingColVat: "VAT",
  billingImportImageUnreadable: "The image could not be read.",
  billingImportMissingName: "Missing name",
  billingImportInvalidPrice: "Invalid price",
  billingImportInvalidVat: "Invalid VAT",
  billingImportReadFailed:
    "We could not read that price list. Try CSV, Excel, PNG, JPEG or WebP.",
  billingImportSaveFailed:
    "The import stopped before every item could be saved.",
  billingImportNotInFile: "Not in this file",
  billingImportTitle: "Import price list",
  billingImportItems: (count: number) => `Import ${count} items`,
  billingImportViewPriceList: "View price list",
  billingImportDropTitle: "Drop a price list here",
  billingImportDropHelp:
    "Excel and CSV files are read instantly in your browser. For a photo or screenshot, alo AI extracts the rows for you to review.",
  billingImportSpreadsheetFormats: "CSV · XLSX",
  billingImportImageFormats: "PNG · JPEG · WebP",
  billingImportChooseFile: "Choose a file",
  billingImportReading: (name: string) => `Reading ${name}…`,
  billingImportRowsFound: (count: number) =>
    `${count} rows found. Review the mapping and exclude anything you do not want.`,
  billingImportReplaceFile: "Replace file",
  billingImportMatchColumns: "Match your columns",
  billingImportSku: "SKU",
  billingImportColumnLabel: (field: string) => `${field} column`,
  billingImportChooseColumn: "Choose a column",
  billingImportColumn: "Import",
  billingImportIncludeRow: (name: string) => `Import ${name}`,
  billingImportRow: (number: number) => `row ${number}`,
  billingImportAlreadyExists: "Already exists",
  billingImportReady: "Ready",
  billingImportComplete: (count: number) =>
    `${count} price-list items imported`,
  billingImportCompleteHelp:
    "They are ready to use in quotes, invoices and shared price connections.",
  colorPickerEyedropper: "Pick a colour from the screen",
  colorPickerHue: "Hue",
  colorPickerChannelValue: (channel: string) => `${channel} value`,
  colorPickerHex: "HEX",
  colorPickerHexColour: "Hex colour",
  colorPickerCopyHex: "Copy hex colour",
  colorPickerUseColour: (colour: string) => `Use ${colour}`,
  colorPickerSaveColour: "Save current colour",
  colorPickerUseDefault: "Use default colour",
  billingEditProductImage: "Edit product image",
  billingCloseImageEditor: "Close image editor",
  billingApplyImage: "Apply image",
  billingPdfPreview: "PDF preview",
  billingQuotationPreview: "Quotation preview",
  billingImagePdfHelp: "This is the image size and crop used in the PDF.",
  billingPdfPaperSizeA4: "A4",
  billingProductPdfPreview: "Product image in the PDF preview",
  billingCropStyle: "Crop style",
  billingFillFrame: "Fill frame",
  billingShowFullImage: "Show full image",
  billingZoom: "Zoom",
  billingCustomZoom: "Custom zoom percentage",
  billingZoomHelp:
    "Use 50–90% to show more of the image, or more than 100% for a tighter crop.",
  billingFocusArea: "Focus area",
  billingCentre: "Centre",
  billingTop: "Top",
  billingBottom: "Bottom",
  billingLeft: "Left",
  billingRight: "Right",
  billingProductImage: "Product image",
  billingProductImageHelp: "Shown beside this item in the customer quotation.",
  billingReplaceImage: "Replace image",
  billingUploadImage: "Upload image",
  billingRemoveImage: "Remove image",
  billingProductDescription: "Product description",
  billingProductDescriptionPlaceholder:
    "Add specifications, materials, scope, or other useful details…",
  billingConnectionsSyncNow: "Sync now",
  billingConnectionsConnectSupplier: "Connect supplier prices",
  billingConnectionsConnectPrices: "Connect prices",
  billingConnectionsEasyOption: "Start with the easiest option",
  billingConnectionsEasyOptionHelp:
    "If your supplier uses alo, paste their invitation link. We handle authentication and product fields automatically.",
  billingConnectionsSupplier: "Supplier",
  billingConnectionsSupplierPlaceholder: "Supplier company name",
  billingConnectionsType: "Connection type",
  billingConnectionsChooseConnection: "Choose a connection",
  billingConnectionsInvitationLink: "Invitation link",
  billingConnectionsInvitationHelp:
    "Your supplier creates this from Shared by me in their alo workspace.",
  billingConnectionsInvitationPlaceholder: "Paste the alo invitation link",
  billingConnectionsAccessKey: "Access key",
  billingConnectionsAccessKeyHelp:
    "Kept private and never shown in your customer documents.",
  billingConnectionsAccessKeyPlaceholder: "Paste the key from your supplier",
  billingConnectionsReady: "Connection ready",
  billingConnectionsTestPreview: "Test and preview",
  billingConnectionsSyncApprovals: "Synchronization and approvals",
  billingConnectionsSyncApprovalsHelp:
    "Choose when prices are checked and which changes require approval.",
  billingConnectionsCheckUpdates: "Check for updates",
  billingConnectionsChooseSchedule: "Choose a schedule",
  billingConnectionsApplyChanges: "Apply price changes",
  billingConnectionsChooseApproval: "Choose an approval rule",
  billingConnectionsChangeLimit: "Automatic change limit",
  billingConnectionsChangeLimitHelp:
    "Changes above this percentage wait for approval.",
  billingConnectionsProductMatching: "Product matching",
  billingConnectionsProductMatchingHelp:
    "Control how supplier products are paired with items already in your catalogue.",
  billingConnectionsMatchBy: "Match products by",
  billingConnectionsChooseMatching: "Choose a matching method",
  billingConnectionsNewProducts: "New supplier products",
  billingConnectionsChooseAction: "Choose an action",
  billingConnectionsFieldMapping: "Supplier field mapping",
  billingConnectionsFieldMappingHelp:
    "Enter the field names used by this supplier. alo will suggest them after the first preview.",
  billingConnectionsSkuField: "SKU field",
  billingConnectionsNameField: "Name field",
  billingConnectionsNetPriceField: "Net price field",
  billingConnectionsCurrencyField: "Currency field",
  billingConnectionsCustomHeader: "Custom authentication header",
  billingConnectionsHeaderName: "Header name",
  billingConnectionsHeaderValue: "Header value",
  billingConnectionsHeaderValuePlaceholder: "Enter the secure value",
  billingConnectionsSharePrices: "Share my prices",
  billingConnectionsCreateSecure: "Create secure connection",
  billingConnectionsYouControl: "You control exactly what this client receives",
  billingConnectionsYouControlHelp:
    "Internal purchase costs, supplier names and margins are never included.",
  billingConnectionsClientPartner: "Client or partner",
  billingConnectionsCompanyName: "Company name",
  billingConnectionsDeliveryMethod: "How will they connect?",
  billingConnectionsChooseDelivery: "Choose a delivery method",
  billingConnectionsPricesToShare: "Prices to share",
  billingConnectionsChoosePrices: "Choose prices",
  billingConnectionsChooseProducts: "Choose price-list products",
  billingConnectionsSearchPriceList: "Search your price list",
  billingConnectionsNoProducts: "No price-list products match this search.",
  billingConnectionsLoadingPriceList: "Loading your price list…",
  billingConnectionsSecureCreated: "Secure price connection created",
  billingConnectionsSendTo: (company: string) =>
    `Send this to ${company}. Access can be paused or revoked at any time.`,
  billingConnectionsKeyShownOnce:
    "The full key is shown only when it is created.",
  billingConnectionsCopy: "Copy",
  billingConnectionsConnected: "Connected",
  billingConnectionsExpired: "Expired",
  billingConnectionsActionNeeded: "Action needed",
  billingConnectionsPaused: "Paused",
  billingConnectionsIndustrialComponentsEur: "Industrial components · EUR",
  billingConnectionsChangesReady: (count: number) =>
    `${count} price changes are ready to review`,
  billingConnectionsUpdatedMinutesAgo: (count: number) =>
    `Updated ${count} minutes ago`,
  billingConnectionsDaily: "Daily",
  billingConnectionsMetalsSheetEur: "Metals and sheet stock · EUR",
  billingConnectionsSupplierRenew:
    "The supplier needs to renew this connection",
  billingConnectionsUpdatedDaysAgo: (count: number) =>
    `Last updated ${count} days ago`,
  billingConnectionsWholesaleContract: "Wholesale catalogue · Contract pricing",
  billingConnectionsWorkspaceReceivesApproved:
    "Their alo workspace receives approved price changes",
  billingConnectionsUsedHoursAgo: (count: number) => `Used ${count} hour ago`,
  billingConnectionsOnApproval: "On approval",
  billingConnectionsProjectSupplyEur: "Project supply prices · EUR",
  billingConnectionsApiExpiryDemo:
    "External API access expires 30 September 2026",
  billingConnectionsUsedYesterday: "Used yesterday",
  billingConnectionsLive: "Live",
  billingConnectionsSupplierCatalogueEur: "Supplier catalogue · EUR",
  billingConnectionsNoChangesAttention: "No price changes need your attention",
  billingConnectionsConnectedNow: "Connected just now",
  billingConnectionsHourly: "Hourly",
  billingConnectionsWeekly: "Weekly",
  billingConnectionsManual: "Manual",
  billingConnectionsLivePriceListAutomatic:
    "Live price list · Automatically updated",
  billingConnectionsSelectedPriceItems: "Selected price-list items",
  billingConnectionsWaitingClient: "Waiting for the client to accept in alo",
  billingConnectionsExternalReady: "External API access is ready to share",
  billingConnectionsCreatedNow: "Created just now",
  billingConnectionsReceivedByMe: "Received by me",
  billingConnectionsSharedByMe: "Shared by me",
  billingConnectionsUpdatedNow: "Updated just now",
  billingConnectionsUpToDate: (company: string) => `${company} is up to date.`,
  billingConnectionsNowSupplying: (company: string) =>
    `${company} is now supplying prices to this workspace.`,
  billingConnectionsNowReceiving: (company: string) =>
    `${company} is now receiving prices from this workspace.`,
  billingConnectionsDisconnectTitle: "Disconnect price connection?",
  billingConnectionsDisconnectReceived: (company: string) =>
    `${company} will stop sending supplier prices to this workspace. Existing prices will be kept, but they will no longer update automatically.`,
  billingConnectionsDisconnectShared: (company: string) =>
    `${company} will stop receiving prices from this workspace. Existing prices will be kept, but they will no longer update automatically.`,
  billingConnectionsDisconnect: "Disconnect",
  billingConnectionsKeepConnected: "Keep connected",
  billingConnectionsTitle: "Price connections",
  billingPriceConnections: "Price connections",
  billingVat: "VAT",
  billingConnectionsSubtitle:
    "Receive live supplier costs and securely share selected selling prices with your clients.",
  billingConnectionsDirection: "Price connection direction",
  billingConnectionsSearch: "Search connections",
  billingConnectionsDismiss: "Dismiss",
  billingConnectionsNoMatches: "No matching connections",
  billingConnectionsNoMatchesHelp:
    "Try another search or create a new price connection.",
  quoteStudioScanToSave: "Scan to save",
  quoteStudioBuildTitle: "Build your quotation",
  quoteStudioBuildHelp: "Add content directly. Changes save automatically.",
  quoteStudioCompanyLogo: "Company logo",
  quoteStudioAddress: "Address",
  quoteStudioContact: "Contact",
  quoteStudioVatId: "VAT ID",
  quoteStudioCompanyNumber: "Company number",
  quoteStudioQuotation: "Quotation",
  quoteStudioPreparedFor: "Prepared for",
  quoteStudioIssued: "Issued",
  quoteStudioValidUntil: "Valid until",
  quoteStudioEditHeader: "Edit header",
  quoteStudioTableName: "Table name",
  quoteStudioPricingTable: "Pricing table",
  quoteStudioTableSettings: "Table settings",
  quoteStudioEditBlock: "Edit block",
  quoteStudioMoveUp: "Move up",
  quoteStudioMoveDown: "Move down",
  quoteStudioDuplicate: "Duplicate",
  quoteStudioDelete: "Delete",
  quoteStudioHeadingLevel: "Heading level",
  quoteStudioHeading1: "Heading 1",
  quoteStudioHeading2: "Heading 2",
  quoteStudioHeading3: "Heading 3",
  quoteStudioSectionHeading: "Section heading",
  quoteStudioParagraph: "Paragraph",
  quoteStudioWriteParagraph: "Write a paragraph…",
  quoteStudioImportantStatement: "Add a customer quote or important statement…",
  quoteStudioAttribution: "Attribution (optional)",
  quoteStudioQuoteAttribution: "Quote attribution",
  quoteStudioSectionText: "Section text",
  quoteStudioSectionTextPlaceholder:
    "Write the information your customer needs…",
  quoteStudioListLayout: "List layout",
  quoteStudioListLayoutHelp: "Split longer lists into easy-to-scan columns.",
  quoteStudioColumns: "Columns",
  quoteStudioChooseColumns: "Choose columns",
  quoteStudioWriteItem: "Write an item",
  quoteStudioMoveItemUp: "Move item up",
  quoteStudioMoveItemDown: "Move item down",
  quoteStudioRemoveItem: "Remove item",
  quoteStudioAddItemBelow: "Add item below",
  quoteStudioListFormatting: "List item formatting",
  quoteStudioBold: "Bold",
  quoteStudioItalic: "Italic",
  quoteStudioEditContentBlock: "Edit content block",
  quoteStudioChangesImmediate:
    "Changes are shown immediately in the quotation.",
  quoteStudioDone: "Done",
  quoteStudioComposeImageText: "Compose image and text",
  quoteStudioComposeImageTextHelp:
    "Arrange the block once and see exactly how it will appear in the quotation.",
  quoteStudioLayoutTools: "Layout tools",
  quoteStudioLayoutToolsHelp:
    "Choose how this content block will appear in the quotation.",
  quoteStudioComposition: "Composition",
  quoteStudioImageFrame: "Image frame",
  quoteStudioFit: "Fit",
  quoteStudioImage: "Image",
  quoteStudioImageDescriptionPlaceholder:
    "Explain the product, project, or result shown in the image.",
  quoteStudioCaption: "Caption",
  quoteStudioCaptionPlaceholder: "Optional short caption",
  quoteStudioTextTools: "Text tools",
  quoteStudioTextFormatting: "Text formatting",
  quoteStudioBulletList: "Bullet list",
  quoteStudioNumberedList: "Numbered list",
  quoteStudioColumnWidth: "Column width",
  quoteStudioSideBySideOnly: "Side-by-side only",
  quoteStudioZoom: "Zoom",
  quoteStudioReset: "Reset",
  quoteStudioZoomOut: "Zoom out",
  quoteStudioZoomIn: "Zoom in",
  quoteStudioInformationTable: "Information table",
  quoteStudioInformationTableHelp:
    "Rename columns, then add as many rows or columns as the document needs.",
  quoteStudioTableColumnCount: "Number of table columns",
  quoteStudioRowActions: "Row actions",
  quoteStudioEnterValue: "Enter value",
  quoteStudioAddFirstRow: "Add the first row to begin this table.",
  quoteStudioAddRowBelow: "Add row below",
  quoteStudioAddContentA11y: "Add quotation content",
  quoteStudioAddContentBelow: "Add content below",
  quoteStudioAddContent: "Add content",
  quoteStudioAddToQuotation: "Add to quotation",
  quoteStudioAddToQuotationHelp:
    "Choose what should appear next in the document.",
  quoteStudioCloseBlockPicker: "Close block picker",
  quoteStudioSearchBlocks: "Search blocks…",
  quoteStudioSearchBlocksA11y: "Search quotation blocks",
  quoteStudioNoMatchingBlocks: "No matching blocks",
  quoteStudioTryAnotherName: "Try another name.",
  quoteStudioFirstBlockHelp:
    "Add text, a heading, or an image as the first block.",
  quoteStudioClose: "Close",
  quoteStudioBrandMark: "Brand mark",
  quoteStudioBrandMarkHelp: "Shown at the top of the customer quotation.",
  quoteStudioQuoteLogo: "Quotation logo",
  quoteStudioUploadLogo: "Upload your logo",
  quoteStudioRemove: "Remove",
  quoteStudioQrTitle: "Contact QR code",
  quoteStudioQrHelp:
    "Let customers scan and save your company contact details.",
  quoteStudioShowQr: "Show contact QR code",
  quoteStudioPlacement: "Placement",
  quoteStudioPlacementHelp:
    "Choose where the code sits beside your company details.",
  quoteStudioSize: "Size",
  quoteStudioSizeHelp: "Preview the QR footprint in the quotation header.",
  quoteStudioQrColour: "QR code colour",
  quoteStudioCompanyInformation: "Company information",
  quoteStudioCompanyLinkedHelp:
    "These values come from Billing → Your details.",
  quoteStudioOverrideHelp:
    "Editing one creates an override for this quotation.",
  quoteStudioUseYourDetails: "Use Your details",
  quoteStudioLinkedYourDetails: "Linked to Your details",
  quoteStudioCompanyName: "Company name",
  quoteStudioCompanyNamePlaceholder: "Your company name",
  quoteStudioWebsite: "Website",
  quoteStudioWebsitePlaceholder: "www.company.com",
  quoteStudioEmail: "Email",
  quoteStudioEmailPlaceholder: "sales@company.com",
  quoteStudioPhone: "Phone",
  quoteStudioVatPlaceholder: "VAT registration number",
  quoteStudioCompanyNumberPlaceholder: "Company registration number",
  quoteStudioCustomerInformation: "Customer information",
  quoteStudioCustomerInformationHelp:
    "Shown beneath Prepared for in the quotation header.",
  quoteStudioCustomerOverrideHelp:
    "Editing a value creates an override for this quotation only.",
  quoteStudioUseSelectedCustomer: "Use selected customer",
  quoteStudioLinkedSelectedCustomer: "Linked to selected customer",
  quoteStudioCustomerCompanyPlaceholder: "Customer company name",
  quoteStudioContactPerson: "Contact person",
  quoteStudioContactNamePlaceholder: "Contact name",
  quoteStudioCustomerEmailPlaceholder: "contact@customer.com",
  quoteStudioCustomerVatPlaceholder: "Customer VAT registration number",
  quoteStudioOnFinalization: "On finalization",
  quoteStudioDaysAfterIssue: (days: string) => `${days} days after issue`,
  quoteStudioSupportingText: "Supporting text",
  quoteStudioHeading: "Heading",
  quoteStudioHeadingHelp: "Choose H1, H2, or H3",
  quoteStudioQuote: "Quote",
  quoteStudioParagraphHelp: "Add explanatory text",
  quoteStudioQuoteHelp: "Highlight a statement",
  quoteStudioBulletListHelp: "List key points",
  quoteStudioNumberedListHelp: "Show ordered steps",
  quoteStudioImageHelp: "Upload and arrange a visual",
  quoteStudioPricingTableHelp: "Group products and services",
  quoteStudioTable: "Table",
  quoteStudioTableHelp: "Create flexible rows and columns",
  quoteStudioDivider: "Divider",
  quoteStudioDividerHelp: "Separate document sections",
  quoteStudioDividerSettings: "Divider settings",
  quoteStudioDividerAppearance: "Divider appearance",
  quoteStudioDividerAppearanceHelp:
    "Choose how this divider appears in the customer quotation.",
  quoteStudioDividerStyle: "Line style",
  quoteStudioDividerStyleHelp:
    "Choose how the divider line appears in your quotation.",
  quoteStudioDividerSolid: "Solid",
  quoteStudioDividerDashed: "Dashed",
  quoteStudioDividerDotted: "Dotted",
  quoteStudioDividerThickness: "Line thickness",
  quoteStudioDividerThicknessHelp: "Select the line thickness.",
  quoteStudioDividerFine: "Fine",
  quoteStudioDividerMedium: "Medium",
  quoteStudioDividerBold: "Bold",
  quoteStudioDividerWidth: "Line width",
  quoteStudioDividerWidthHelp: "Set the divider line width.",
  quoteStudioDividerColour: "Line colour",
  quoteStudioChooseDividerColour: "Choose divider colour",
  quoteStudioChooseColour: "Choose colour",
  quoteStudioHexColour: "Hex colour",
  quoteStudioCopyColour: "Copy colour",
  quoteStudioCategoryText: "Text",
  quoteStudioEditQuotationHeader: "Edit quotation header",
  quoteStudioCustomizeQuotation: "Customize quotation",
  quoteStudioChangesSavedAutomatically: "Changes are saved automatically.",
  quoteStudioReplace: "Replace",
  quoteStudioChooseFile: "Choose file",
  quoteStudioLeft: "Left",
  quoteStudioRight: "Right",
  quoteStudioSmall: "Small",
  quoteStudioMedium: "Medium",
  quoteStudioLarge: "Large",
  quoteStudioQrPlacementA11y: (side: string) => `Place QR code on the ${side}`,
  quoteStudioQrColourHelp: "Choose a dark colour for reliable scanning",
  quoteStudioPhonePlaceholder: "+49 30 123 456",
  quoteStudioAddressPlaceholder:
    "Street and number\nPostal code and city\nCountry",
  quoteStudioHeaderStyle: "Header style",
  quoteStudioHeaderStyleHelp:
    "Choose a professional composition. Your saved company information fills it automatically.",
  quoteStudioHeaderArrangement: "Header arrangement",
  quoteStudioHeaderArrangementHelp:
    "Choose which side carries your company identity.",
  quoteStudioLogoLeft: "Logo left",
  quoteStudioLogoRight: "Logo right",
  quoteStudioLogoLeftHelp:
    "Company identity on the left; quotation details opposite.",
  quoteStudioLogoRightHelp:
    "Company identity on the right; quotation details opposite.",
  quoteStudioColumnBalance: "Column balance",
  quoteStudioColumnBalanceHelp:
    "Choose how much space the company and customer sides receive.",
  quoteStudioColumnBalanceA11y: "Quotation header column balance",
  quoteStudioColumnRatioA11y: (company: string, customer: string) =>
    `Company ${company} percent, customer ${customer} percent`,
  quoteStudioDocumentPalette: "Document palette",
  quoteStudioDocumentPaletteHelp:
    "Control the customer-facing page and pricing table colours.",
  quoteStudioResetDefaults: "Reset to defaults",
  quoteStudioDocument: "Document",
  quoteStudioDocumentHelp: "Brand, page, header, and copy.",
  quoteStudioAccent: "Accent",
  quoteStudioAccentHelp: "Brand actions and highlights",
  quoteStudioContactIcons: "Contact icons",
  quoteStudioContactIconsHelp: "Email, phone, and website icons",
  quoteStudioPage: "Page",
  quoteStudioPageHelp: "Customer-facing background",
  quoteStudioHeader: "Header",
  quoteStudioHeaderHelp: "Header background",
  quoteStudioText: "Text",
  quoteStudioTextHelp: "Primary text",
  quoteStudioBulletDots: "Bullet dots",
  quoteStudioListMarkers: "List markers",
  quoteStudioNumberMarkers: "Number markers",
  quoteStudioNumberedSteps: "Numbered steps",
  quoteStudioPricingTables: "Pricing tables",
  quoteStudioPricingTablesHelp: "Keep headings and rows easy to scan.",
  quoteStudioTableHeading: "Table heading",
  quoteStudioTableHeadingHelp: "Table header background",
  quoteStudioTableRows: "Table rows",
  quoteStudioTableRowsHelp: "Default row background",
  quoteStudioTypography: "Typography",
  quoteStudioTypographyHelp:
    "Choose the reading style that best matches your brand.",
  quoteStudioProposal: "Proposal",
  quoteStudioCloseTableSettings: "Close table settings",
  quoteStudioTableChangesSavedAutomatically:
    "Table changes are saved automatically.",
  quoteStudioChooseLayout: "Choose a layout",
  quoteStudioChooseLayoutHelp:
    "Select a starting point, then adjust the visible content and columns below.",
  quoteStudioCompact: "Compact",
  quoteStudioCompactHelp: "Names and prices only",
  quoteStudioDetailed: "Detailed",
  quoteStudioDetailedHelp: "Descriptions with optional images",
  quoteStudioCatalogue: "Catalogue",
  quoteStudioCatalogueHelp: "Larger product images and details",
  quoteStudioProductContent: "Product content",
  quoteStudioProductContentHelp:
    "Optional information shown with each product or service.",
  quoteStudioProductImages: "Product images",
  quoteStudioProductImagesHelp: "Upload an image for each table row",
  quoteStudioProductDescriptions: "Product descriptions",
  quoteStudioProductDescriptionsHelp:
    "Add specifications or scope beneath each item",
  quoteStudioVisibleColumns: "Visible columns",
  quoteStudioVisibleColumnsHelp:
    "Product name and quotation total always remain visible.",
  quoteStudioUnit: "Unit",
  quoteStudioQuantity: "Quantity",
  quoteStudioUnitPrice: "Unit price",
  quoteStudioVatRate: "VAT rate",
  quoteStudioLineTotal: "Line total",
  quoteStudioShowColumn: (label: string) =>
    `Show the ${label.toLowerCase()} column`,
  quoteStudioPricingTableTotals: "Pricing table totals",
  quoteStudioPricingTableTotalsHelp:
    "Choose how the amount summary appears beneath each pricing table. Every table keeps its own subtotal.",
  quoteStudioSummaryCard: "Summary card",
  quoteStudioSummaryCardHelp: "Compact and right aligned",
  quoteStudioFullWidth: "Full width",
  quoteStudioFullWidthHelp: "Balances the entire table",
  quoteStudioTableFooter: "Table footer",
  quoteStudioTableFooterHelp: "Feels attached to the rows",
  quoteStudioTotalsStyle: "Total style",
  quoteStudioTotalsStyleName: (
    style: "soft" | "minimal" | "framed" | "accent",
  ) =>
    ({
      soft: "Soft card",
      minimal: "Minimal",
      framed: "Framed",
      accent: "Alo accent",
    })[style],
  quoteStudioAmountDetails: "Amount details",
  quoteStudioTotalOnly: "Total only",
  quoteStudioTotalOnlyHelp: "The shortest summary",
  quoteStudioNetVatTotal: "Net, VAT and total",
  quoteStudioNetVatTotalHelp: "Recommended for most quotations",
  quoteStudioVatBreakdown: "VAT breakdown",
  quoteStudioVatBreakdownHelp: "Show every VAT rate",
  quoteStudioCurrencyCode: "Currency code",
  quoteStudioCurrencyCodeHelp: "Show EUR, USD, or the quotation currency",
  quoteStudioEmphasizeTotal: "Emphasize total",
  quoteStudioEmphasizeTotalHelp: "Give the final amount stronger hierarchy",
  quoteStudioVatNote: "VAT note",
  quoteStudioVatNoteHelp: "Explain that VAT is shown separately",
  quoteStudioListItemFormatting: "List item formatting",
  quoteStudioDraftQuotation: "Draft quotation",
  quoteStudioPricingTableNumber: (number: number) => `Pricing table ${number}`,
  quoteStudioNumberedListColumns: "Numbered list columns",
  quoteStudioBulletListColumns: "Bullet list columns",
  quoteStudioParagraphColumns: "Paragraph columns",
  quoteStudioQuoteColumns: "Quote columns",
  billingDownloadPdf: "Download PDF",
  billingDownloadPdfFailed: "The PDF could not be downloaded.",
  quoteStudioListStyle: "List style",
  quoteStudioNumberingStyle: "Numbering style",
  quoteStudioBulletStyle: "Bullet style",
  quoteStudioChooseListStyle: "Choose a list style",
  quoteStudioIndentItem: "Indent item",
  quoteStudioOutdentItem: "Outdent item",
  quoteStudioListStyleName: (style: string) =>
    ({
      decimal: "Numbers, letters, roman",
      parenthesis: "Numbers with parentheses",
      outline: "Outline (1.1, 1.2.1)",
      "upper-alpha": "Capital letters",
      roman: "Roman numerals",
      "leading-zero": "Leading zeros (01, 02)",
      disc: "Round bullets",
      diamond: "Diamonds and arrows",
      square: "Squares",
      arrow: "Arrows",
      star: "Stars",
      chevron: "Chevrons",
      checkbox: "Checkboxes",
    })[style] ?? style,
  quoteStudioColumnCount: (count: number) =>
    count === 1 ? "1 column" : `${count} columns`,
  quoteStudioNumberedItemA11y: (number: number) => `Numbered item ${number}`,
  quoteStudioBulletItemA11y: (number: number) => `Bullet item ${number}`,
  quoteStudioBelowImage: "Below image",
  quoteStudioImageLeft: "Image left",
  quoteStudioImageRight: "Image right",
  quoteStudioNatural: "Natural",
  quoteStudioWide: "Wide",
  quoteStudioSquare: "Square",
  quoteStudioFillFrame: "Fill frame",
  quoteStudioWholeImage: "Whole image",
  quoteStudioColumnNumber: (number: number) => `Column ${number}`,
  quoteStudioColumnNameA11y: (number: number) => `Column ${number} name`,
  quoteStudioRemoveColumnA11y: (label: string) => `Remove ${label}`,
  quoteStudioRemoveRowA11y: (row: number) => `Remove row ${row}`,
  quoteStudioTableCellA11y: (column: string, row: number) =>
    `${column}, row ${row}`,
  quoteStudioCategoryMedia: "Media",
  quoteStudioCategoryTables: "Tables",
  quoteStudioCategoryLayout: "Layout",
  quoteStudioSearchResults: "Search results",
  quoteStudioDesignDatabaseError:
    "The quotation design database could not be opened.",
  quoteStudioDesignSaveError: "The quotation design could not be saved.",
  quoteStudioDesignSaveCancelled: "Saving the quotation design was cancelled.",
  quoteStudioDesignSaveRetry:
    "This design could not be saved. Try a smaller image or upload it again.",
  quoteStudioShowSubtotal: "Show subtotal",
  quoteStudioHideSubtotal: "Hide subtotal",
  quoteStudioQuotationImageAlt: "Quotation image",
  quoteStudioNoProposalContent: "No proposal content",
  quoteStudioStartQuotationBelow: "Start your quotation below",
  billingExitPreview: "Exit preview",
  quoteStudioModern: "Modern",
  quoteStudioModernHelp: "Clean and confident",
  quoteStudioEditorial: "Editorial",
  quoteStudioEditorialHelp: "Story-led headings",
  quoteStudioMinimal: "Minimal",
  quoteStudioMinimalHelp: "Quiet and precise",
  quoteStudioSignature: "Signature",
  quoteStudioSignatureHelp: "Balanced identity and quotation details",
  quoteStudioHeaderEditorialHelp: "A confident, title-led opening",
  quoteStudioBrandBand: "Brand band",
  quoteStudioBrandBandHelp: "A stronger branded introduction",
  quoteStudioHeaderMinimalHelp: "Quiet, compact and precise",
  quoteStudioLogoStack: "Logo stack",
  quoteStudioLogoStackHelp: "Company name beneath the logo",
  billingVatIncludedNote: "VAT is included in the total.",
  billingVatSeparateNote: "VAT is shown separately from the net amount.",
  billingPricingTableEditorHelp:
    "Add, edit, remove, or drag products and services into the right order.",
  billingPricingTableEmptyHelp:
    "Add a product or service to begin this pricing table.",
  billingImage: "Image",
  billingQuoteExitPreviewToEdit: "Exit preview to edit this quotation",
  billingQuoteEditContent: "Edit quotation content",
  billingQuoteCreateRevision:
    "Create a revision to edit this finalized quotation",
  billingQuoteEdit: "Edit quote",
  billingQuoteExitPreviewToCustomize:
    "Exit preview to customize this quotation",
  billingQuoteCreateRevisionToCustomize:
    "Create a revision to customize this finalized quotation",
  billingQuoteCreateRevisionTitle: "Create an editable revision?",
  billingQuoteCreateRevisionConfirm:
    "The finalized quotation will stay unchanged. alo will create one new draft with the same customer, content, prices, and design.",
  billingQuoteCreateRevisionAction: "Create revision",
  billingConnectionsProductCount: (count: number) =>
    count === 1 ? "1 product" : `${count} products`,
  billingConnectionsUpdateCadence: (cadence: string) => `${cadence} updates`,
  billingConnectionsViaAlo: "Connected through alo",
  billingConnectionsExternalApi: "External API",
  billingConnectionsReviewChanges: (count: number) => `Review ${count} changes`,
  billingConnectionsResume: "Resume",
  billingConnectionsPause: "Pause",
  billingConnectionsDisconnectCompany: (company: string) =>
    `Disconnect ${company}`,
  billingConnectionsSpreadsheetFeed: "Spreadsheet or feed",
  billingConnectionsPriceApiAddress: "Price API address",
  billingConnectionsFeedAddress: "Feed address",
  billingConnectionsFormatDetection:
    "alo detects JSON, CSV, and spreadsheet feeds automatically.",
  billingConnectionsAddressPlaceholder: "https://supplier.example/prices",
  billingConnectionsAdvancedSettings: "Advanced settings",
  billingConnectionsEveryHour: "Every hour",
  billingConnectionsOnceDay: "Once a day",
  billingConnectionsOnceWeek: "Once a week",
  billingConnectionsManualSync: "Only when I sync",
  billingConnectionsReviewEveryChange: "Review every change",
  billingConnectionsAutomaticLimited: "Apply automatically within a limit",
  billingConnectionsAutomaticAll: "Apply all changes automatically",
  billingConnectionsMatchSku: "SKU, then barcode and name",
  billingConnectionsMatchBarcode: "Barcode, then SKU and name",
  billingConnectionsMatchName: "Product name",
  billingConnectionsMatchReview: "Review every match",
  billingConnectionsHoldReview: "Hold for review",
  billingConnectionsCreateDraftItems: "Create draft price-list items",
  billingConnectionsDoNotImport: "Do not import",
  billingConnectionsHeaderNamePlaceholder: "X-API-Key",
  billingConnectionsAloInvitationLink: "alo invitation link",
  billingConnectionsExternalPricingApi: "External pricing API",
  billingConnectionsTestSummary: (
    found: number,
    matched: number,
    review: number,
  ) =>
    `${found} products found · ${matched} matched automatically · ${review} can be reviewed after connecting.`,
  billingConnectionsCustomHeaderHelp:
    "Optional. Use this only when the supplier's documentation requires a header other than the access key above.",
  billingConnectionsInviteAloWorkspace: "Invite their alo workspace",
  billingConnectionsGiveExternalApi: "Give them external API access",
  billingConnectionsLivePriceListActive: (count: number) =>
    `Live price list · ${count} active products`,
  billingConnectionsChooseProductsSelected: (count: number) =>
    `Choose products · ${count} selected`,
  billingConnectionsItemUnit: "item",
  billingConnectionsPrices: "Prices",
  billingConnectionsUpdates: "Updates",
  billingConnectionsValidity: "Validity",
  billingConnectionsLivePriceListCount: (count: number) =>
    `Live price list (${count})`,
  billingConnectionsSelectedProductsCount: (count: number) =>
    `${count} selected products`,
  billingConnectionsChangesFlow:
    "Price-list changes flow through this connection",
  billingConnectionsNoExpiry: "No expiry",
} as const;

/** Every string key in the catalog. */
export type StringKey = keyof typeof en;

/**
 * The catalog shape a locale must satisfy, with the English literal
 * value types widened: plain entries become `string`, interpolation
 * entries keep their exact function signature. Locale catalogs are
 * `Partial<Catalog>` — any key they omit falls back to English, so a
 * partial translation degrades gracefully (never a blank label).
 */
export type Catalog = {
  [K in keyof typeof en]: (typeof en)[K] extends (...args: infer A) => infer R
    ? (...args: A) => R
    : string;
};
