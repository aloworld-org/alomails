// Test-environment setup, loaded before every suite (`vite.config.ts` →
// `test.setupFiles`).
//
// ## Why this file exists: the gate was lying about which tests were broken
//
// Testing Library's async helpers — `findBy*`, `waitFor` — poll until an
// element appears and give up after **one second** by default. That is a
// generous budget on an idle laptop and a tight one on a shared CI runner
// building three other jobs beside you. The failure it produces is
// indistinguishable from a real one: "Unable to find an element with the
// text…", pointing at a component that is perfectly correct and simply had not
// rendered yet.
//
// The evidence that this was happening rather than a real defect: three
// different tests failed on three consecutive runs of the same commit — one in
// CI (`an agent's message is marked as an agent`), two locally while a Docker
// build was running (`showing earlier messages…`, `the hiring board draws the
// stages…`) — and every one of them passed when its file was run on its own.
// A gate that names a different innocent test each time is worse than no gate,
// because the habit it teaches is to re-run until green.
//
// Five seconds is not a licence for slow code. Nothing here talks to a network
// or a database — the fetch is faked in every suite — so a component that
// genuinely never renders still fails, just five seconds later instead of one.
// What the extra budget buys is the difference between "this is broken" and
// "the machine was busy", which is the only thing the old timeout was
// measuring.
import { configure } from "@testing-library/dom";

configure({ asyncUtilTimeout: 5000 });
