import { match } from "@formatjs/intl-localematcher";
import { routing } from "./routing";

export function resolveRequestLocale(cookieLocale: string | undefined, acceptLanguage: string | null): string {
  if (cookieLocale && routing.locales.includes(cookieLocale as (typeof routing.locales)[number])) {
    return cookieLocale;
  }

  return match(parseAcceptLanguage(acceptLanguage), routing.locales, routing.defaultLocale);
}

function parseAcceptLanguage(acceptLanguage: string | null): string[] {
  if (!acceptLanguage) {
    return [];
  }

  return acceptLanguage
    .split(",")
    .map((part, index) => {
      const [language, ...params] = part.trim().split(";");
      const quality = params
        .map((param) => param.trim())
        .find((param) => param.startsWith("q="))
        ?.slice(2);
      return {
        language,
        quality: quality === undefined ? 1 : Number(quality),
        index
      };
    })
    .filter(({ language, quality }) => language && Number.isFinite(quality))
    .sort((left, right) => right.quality - left.quality || left.index - right.index)
    .map(({ language }) => language);
}
