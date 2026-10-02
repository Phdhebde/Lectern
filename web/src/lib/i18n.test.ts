import { describe, expect, it } from "vitest";
import fr from "../i18n/fr.json";
import en from "../i18n/en.json";
import { deepMerge, interpolate, lookup } from "./i18n";

function keys(obj: object, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    typeof v === "object" ? keys(v, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

describe("i18n", () => {
  it("has the same keys in every bundled language", () => {
    expect(keys(fr).sort()).toEqual(keys(en).sort());
  });

  it("interpolates placeholders and keeps unknown ones", () => {
    expect(interpolate("Hello {name} on {instance}", { name: "Ada" })).toBe("Hello Ada on {instance}");
  });

  it("lets instance overrides win", () => {
    const merged = deepMerge({ a: { b: "x", c: "y" } }, { a: { b: "z" } });
    expect(lookup(merged, "a.b")).toBe("z");
    expect(lookup(merged, "a.c")).toBe("y");
  });
});
