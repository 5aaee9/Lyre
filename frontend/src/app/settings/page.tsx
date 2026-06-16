"use client";

import { useState } from "react";
import Link from "next/link";
import { SettingsDialog } from "@/components/settings-dialog";

export default function SettingsPage() {
  const [open, setOpen] = useState(true);

  return (
    <section className="grid gap-4">
      <Link className="text-sm text-[#1f6f50]" href="/">
        Back to rooms
      </Link>
      <SettingsDialog open={open} onOpenChange={setOpen} />
    </section>
  );
}
