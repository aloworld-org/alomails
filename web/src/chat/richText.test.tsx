// The reading end of a message. The rules matter most where they must NOT
// fire: a body is text somebody typed, and a renderer that reformats code or
// mangles a filename is worse than one that does nothing.
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

// Without this each render stacks in the same document, and an assertion
// meant for one body silently reads several.
afterEach(cleanup);

import { renderBody } from "./richText";

/** The mention marker the module passes in; here, plain text. */
const plain = (t: string) => [t];

function show(body: string) {
  return render(<div data-testid="out">{renderBody(body, plain)}</div>);
}

describe("reading a message body", () => {
  test("the everyday marks", () => {
    const { container } = show("**bold** _italic_ ~~gone~~ `code`");
    expect(container.querySelector("strong")?.textContent).toBe("bold");
    expect(container.querySelector("em")?.textContent).toBe("italic");
    expect(container.querySelector("s")?.textContent).toBe("gone");
    expect(container.querySelector("code")?.textContent).toBe("code");
  });

  test("code wins over everything inside it", () => {
    // Otherwise `**not bold**` in a snippet silently loses its asterisks, and
    // the reader is shown something the sender did not write.
    const { container } = show("`**not bold**`");
    expect(container.querySelector("strong")).toBeNull();
    expect(container.querySelector("code")?.textContent).toBe("**not bold**");
  });

  test("an underscore inside a word is not italics", () => {
    // snake_case_names are ordinary in a workplace; eating the underscores
    // would corrupt a filename or a column name in the middle of a sentence.
    const { container } = show("see user_id_column in the table");
    expect(container.querySelector("em")).toBeNull();
    expect(container.textContent).toContain("user_id_column");
  });

  test("a fenced block keeps its text exactly", () => {
    const { container } = show("```rust\nfn main() {}\n```");
    const pre = container.querySelector("pre");
    expect(pre).not.toBeNull();
    expect(pre?.textContent).toContain("fn main() {}");
  });

  test("lists and quotes", () => {
    const { container } = show("- one\n- two\n\n1. first\n\n> quoted");
    expect(container.querySelectorAll("ul li")).toHaveLength(2);
    expect(container.querySelectorAll("ol li")).toHaveLength(1);
    expect(container.querySelector("blockquote")?.textContent).toBe("quoted");
  });

  test("links open safely", () => {
    const { container } = show("see https://alomails.com for more");
    const a = container.querySelector("a");
    expect(a?.getAttribute("href")).toBe("https://alomails.com");
    // A workplace link must not hand the destination this workspace's URL.
    expect(a?.getAttribute("rel")).toContain("noreferrer");
  });

  test("maths renders, and broken maths stays readable", () => {
    const { container } = show("$e^{i\pi}+1=0$");
    expect(container.querySelector(".katex")).not.toBeNull();
    // Nonsense LaTeX must not blank the message or throw — the sender's own
    // characters are more use than an error nobody can act on.
    const bad = show("$\frac{$");
    expect(bad.container.textContent).toContain("\frac");
  });

  test("a plain body is left alone", () => {
    const { container } = show("just a normal sentence, 3 * 4 = 12 and a_b");
    expect(container.textContent).toBe(
      "just a normal sentence, 3 * 4 = 12 and a_b",
    );
  });
});
