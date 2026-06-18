"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { Headphones, SlidersHorizontal, UserRound, Waves } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle
} from "@/components/ui/dialog";
import {
  AdvancedSettings,
  DeviceSettings,
  NoiseSettings,
  ProfileSettings
} from "@/components/settings-tabs";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { readSettingsSnapshot, useSettingsStore, type SettingsSnapshot } from "@/lib/settings-store";

type SettingsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave?: (settings: SettingsSnapshot) => void | Promise<void>;
};

export function SettingsDialog({ open, onOpenChange, onSave }: SettingsDialogProps) {
  const t = useTranslations("Settings");
  const router = useRouter();
  const nickname = useSettingsStore((state) => state.nickname);
  const language = useSettingsStore((state) => state.language);
  const audioDiagnosticsEnabled = useSettingsStore((state) => state.audioDiagnosticsEnabled);
  const noise = useSettingsStore((state) => state.noise);
  const audioProcessing = useSettingsStore((state) => state.audioProcessing);
  const audioDevices = useSettingsStore((state) => state.audioDevices);
  const setNickname = useSettingsStore((state) => state.setNickname);
  const setLanguage = useSettingsStore((state) => state.setLanguage);
  const setAudioDiagnosticsEnabled = useSettingsStore((state) => state.setAudioDiagnosticsEnabled);
  const setNoise = useSettingsStore((state) => state.setNoise);
  const setAudioProcessing = useSettingsStore((state) => state.setAudioProcessing);
  const setAudioDevices = useSettingsStore((state) => state.setAudioDevices);
  const [mediaDevices, setMediaDevices] = useState<MediaDeviceInfo[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open) {
      return;
    }
    let cancelled = false;
    void navigator.mediaDevices?.enumerateDevices?.()
      .then((devices) => {
        if (!cancelled) {
          setMediaDevices(devices);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setMediaDevices([]);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  async function save() {
    setSaving(true);
    try {
      await onSave?.(readSettingsSnapshot());
      onOpenChange(false);
      router.refresh();
    } finally {
      setSaving(false);
    }
  }

  const microphones = mediaDevices.filter((device) => device.kind === "audioinput");
  const speakers = mediaDevices.filter((device) => device.kind === "audiooutput");

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[calc(100vh-2rem)] overflow-hidden p-0 sm:max-w-2xl">
        <div className="grid max-h-[calc(100vh-2rem)] grid-rows-[auto_minmax(0,1fr)_auto]">
          <DialogHeader className="px-4 pt-4 sm:px-5 sm:pt-5">
            <DialogTitle>{t("title")}</DialogTitle>
            <DialogDescription>{t("description")}</DialogDescription>
          </DialogHeader>

          <Tabs defaultValue="profile" className="min-h-0 gap-0 overflow-hidden">
            <div className="border-y border-lyre-subtle-border px-4 py-3 sm:px-5">
              <TabsList className="grid h-auto w-full grid-cols-2 gap-1 md:grid-cols-4">
                <TabsTrigger value="profile">
                  <UserRound aria-hidden="true" className="size-4" />
                  <span>{t("profile")}</span>
                </TabsTrigger>
                <TabsTrigger value="noise">
                  <Waves aria-hidden="true" className="size-4" />
                  <span>{t("noise")}</span>
                </TabsTrigger>
                <TabsTrigger value="devices">
                  <Headphones aria-hidden="true" className="size-4" />
                  <span>{t("devices")}</span>
                </TabsTrigger>
                <TabsTrigger value="advanced">
                  <SlidersHorizontal aria-hidden="true" className="size-4" />
                  <span>{t("advanced")}</span>
                </TabsTrigger>
              </TabsList>
            </div>

            <div className="min-h-0 overflow-y-auto px-4 py-4 sm:px-5">
              <TabsContent value="profile">
                <ProfileSettings
                  language={language}
                  nickname={nickname}
                  setLanguage={setLanguage}
                  setNickname={setNickname}
                />
              </TabsContent>

              <TabsContent value="noise">
                <NoiseSettings noise={noise} setNoise={setNoise} />
              </TabsContent>

              <TabsContent value="devices">
                <DeviceSettings
                  audioDevices={audioDevices}
                  microphones={microphones}
                  setAudioDevices={setAudioDevices}
                  speakers={speakers}
                />
              </TabsContent>

              <TabsContent value="advanced">
                <AdvancedSettings
                  audioDiagnosticsEnabled={audioDiagnosticsEnabled}
                  audioProcessing={audioProcessing}
                  setAudioDiagnosticsEnabled={setAudioDiagnosticsEnabled}
                  setAudioProcessing={setAudioProcessing}
                />
              </TabsContent>
            </div>
          </Tabs>

          <DialogFooter className="m-0">
            <Button disabled={saving} onClick={save}>
              {saving ? t("saving") : t("save")}
            </Button>
          </DialogFooter>
        </div>
      </DialogContent>
    </Dialog>
  );
}
