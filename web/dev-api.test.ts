import { describe, expect, it } from "vitest";
import { resolveDevApi } from "./dev-api";

describe("resolveDevApi", () => {
  it("uses the local API when no override is configured", () => {
    expect(resolveDevApi(undefined)).toBe("http://localhost:8080");
    expect(resolveDevApi("   ")).toBe("http://localhost:8080");
  });

  it("uses an explicit development API override", () => {
    expect(resolveDevApi(" https://api.example.test ")).toBe(
      "https://api.example.test",
    );
  });
});
