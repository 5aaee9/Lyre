import { getRequestConfig } from "next-intl/server";
import { cookies, headers } from "next/headers";
import { resolveRequestLocale } from "./request-locale";

export default getRequestConfig(async () => {
  const cookieStore = await cookies();
  const headerStore = await headers();
  const locale = resolveRequestLocale(
    cookieStore.get("NEXT_LOCALE")?.value,
    headerStore.get("accept-language")
  );

  return {
    locale,
    messages: (await import(`../../messages/${locale}.json`)).default
  };
});
