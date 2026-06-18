import { describe, expect, it } from "vitest";
import { resolveRequestLocale } from "./request-locale";

describe("resolveRequestLocale", () => {
  it("prefers a supported locale cookie", () => {
    expect(resolveRequestLocale("zh-CN", "en-US,en;q=0.9")).toBe("zh-CN");
  });

  it("falls back to Accept-Language when the locale cookie is missing", () => {
    expect(resolveRequestLocale(undefined, "zh-CN,zh;q=0.9,en-US;q=0.8")).toBe("zh-CN");
  });

  it("uses the default locale when no supported request language is found", () => {
    expect(resolveRequestLocale("fr-FR", "fr-FR,fr;q=0.9")).toBe("en-US");
  });
});
