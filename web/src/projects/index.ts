// Public surface of the Projects area. The product surface mounts the module
// and the rail widget; nothing outside reaches into the views, the dialogs or
// the API client.
export { ProjectsModule } from "./ProjectsModule";

// The running-timer widget lives in the rail rather than in the module, because
// a timer you cannot see from your inbox is a timer you forget to stop. It is
// declared by the workspace product surface, so the standalone mail product —
// which has no Projects — never renders or imports it.
export { TimerWidget } from "./TimerWidget";
