import type { Metadata } from "next";
import Link from "next/link";
import Script from "next/script";
import "./globals.css";
import { Geist } from "next/font/google";
import { cn } from "@/lib/utils";

const geist = Geist({subsets:['latin'],variable:'--font-sans'});

export const metadata: Metadata = {
  title: "Lyre",
  description: "High performance VOIP rooms"
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  const runtimeConfig = {
    appBaseUrl: process.env.APP_BASE_URL ?? "http://localhost:3000",
    appApiUrl: process.env.APP_API_URL ?? "http://localhost:8080"
  };

  return (
    <html lang="en" className={cn("font-sans", geist.variable)}>
      <head>
        <Script
          id="lyre-runtime-config"
          strategy="beforeInteractive"
          dangerouslySetInnerHTML={{
            __html: `window.__LYRE_CONFIG__=${JSON.stringify(runtimeConfig)};`
          }}
        />
      </head>
      <body>
        <div className="min-h-screen bg-[#f6f8f5] text-[#18211c]">
          <header className="border-b border-[#d8ded6] bg-white">
            <nav className="mx-auto flex max-w-5xl items-center justify-between px-5 py-3">
              <Link className="text-lg font-semibold" href="/">
                Lyre
              </Link>
            </nav>
          </header>
          <main className="mx-auto max-w-5xl px-5 py-6">{children}</main>
        </div>
      </body>
    </html>
  );
}
