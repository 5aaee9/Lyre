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
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { parseNoiseProvider } from "@/lib/api";
import { readSettingsSnapshot, supportedLanguages, useSettingsStore, type SettingsSnapshot } from "@/lib/settings-store";

type SettingsDialogProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSave?: (settings: SettingsSnapshot) => void | Promise<void>;
};

const DEFAULT_DEVICE_VALUE = "default";

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
                <SettingsSection
                  description={t("identityDescription")}
                  title={t("identityTitle")}
                >
                  <FieldRow htmlFor="settings-language" label={t("language")}>
                    <Select value={language} onValueChange={setLanguage}>
                      <SelectTrigger aria-label={t("language")} className="w-full" id="settings-language">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="system">{t("languageSystem")}</SelectItem>
                        {supportedLanguages.map((locale) => (
                          <SelectItem key={locale} value={locale}>
                            {locale}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </FieldRow>
                  <FieldRow htmlFor="settings-nickname" label={t("nickname")}>
                    <Input
                      id="settings-nickname"
                      value={nickname}
                      onChange={(event) => setNickname(event.target.value)}
                      placeholder={t("nicknamePlaceholder")}
                    />
                  </FieldRow>
                </SettingsSection>
              </TabsContent>

              <TabsContent value="noise">
                <SettingsSection
                  description={t("serverNoiseDescription")}
                  title={t("serverNoiseTitle")}
                >
                  <FieldRow htmlFor="settings-server-noise" label={t("provider")}>
                    <Select
                      value={noise.provider}
                      onValueChange={(value) =>
                        setNoise({
                          ...noise,
                          provider: parseNoiseProvider(value)
                        })
                      }
                    >
                      <SelectTrigger aria-label={t("serverNoiseCancelling")} className="w-full" id="settings-server-noise">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="off">Off</SelectItem>
                        <SelectItem value="rnnoise">RNNoise</SelectItem>
                        <SelectItem value="deepfilternet">DeepFilterNet</SelectItem>
                        <SelectItem value="dpdfnet">DPDFNet</SelectItem>
                      </SelectContent>
                    </Select>
                  </FieldRow>

                  {noise.provider === "dpdfnet" ? (
                    <FieldRow htmlFor="settings-dpdfnet-model" label="Model">
                      <Select
                        value={noise.dpdfnet.model}
                        onValueChange={(value) =>
                          setNoise({
                            ...noise,
                            dpdfnet: {
                              model: value
                            }
                          })
                        }
                      >
                        <SelectTrigger aria-label={t("dpdfnetModel")} className="w-full" id="settings-dpdfnet-model">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="baseline">baseline</SelectItem>
                          <SelectItem value="dpdfnet2">dpdfnet2</SelectItem>
                          <SelectItem value="dpdfnet4">dpdfnet4</SelectItem>
                          <SelectItem value="dpdfnet8">dpdfnet8</SelectItem>
                          <SelectItem value="dpdfnet2_48khz_hr">dpdfnet2_48khz_hr</SelectItem>
                          <SelectItem value="dpdfnet8_48khz_hr">dpdfnet8_48khz_hr</SelectItem>
                        </SelectContent>
                      </Select>
                    </FieldRow>
                  ) : null}

                  <FieldRow htmlFor="settings-intensity" label={t("intensity")}>
                    <Input
                      aria-label={t("intensity")}
                      id="settings-intensity"
                      max={1}
                      min={0}
                      step={0.05}
                      type="number"
                      value={noise.intensity}
                      onChange={(event) =>
                        setNoise({
                          ...noise,
                          intensity: Number(event.target.value)
                        })
                      }
                    />
                  </FieldRow>

                  <FieldRow htmlFor="settings-vad" label={t("vad")}>
                    <Input
                      aria-label={t("vadInput")}
                      id="settings-vad"
                      max={1}
                      min={0}
                      step={0.05}
                      type="number"
                      value={noise.voice_activity_threshold}
                      onChange={(event) =>
                        setNoise({
                          ...noise,
                          voice_activity_threshold: Number(event.target.value)
                        })
                      }
                    />
                  </FieldRow>
                </SettingsSection>
              </TabsContent>

              <TabsContent value="devices">
                <SettingsSection
                  description={t("deviceDescription")}
                  title={t("deviceTitle")}
                >
                  <FieldRow htmlFor="settings-microphone" label={t("microphone")}>
                    <Select
                      value={audioDevices.inputDeviceId || DEFAULT_DEVICE_VALUE}
                      onValueChange={(value) =>
                        setAudioDevices({
                          ...audioDevices,
                          inputDeviceId: value === DEFAULT_DEVICE_VALUE ? "" : value
                        })
                      }
                    >
                      <SelectTrigger aria-label={t("microphone")} className="w-full" id="settings-microphone">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value={DEFAULT_DEVICE_VALUE}>{t("microphoneDefault")}</SelectItem>
                        {microphones.map((device) => (
                          <SelectItem key={device.deviceId} value={device.deviceId}>
                            {device.label || t("microphone")}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </FieldRow>

                  <FieldRow htmlFor="settings-speaker" label={t("speaker")}>
                    <Select
                      value={audioDevices.outputDeviceId || DEFAULT_DEVICE_VALUE}
                      onValueChange={(value) =>
                        setAudioDevices({
                          ...audioDevices,
                          outputDeviceId: value === DEFAULT_DEVICE_VALUE ? "" : value
                        })
                      }
                    >
                      <SelectTrigger aria-label={t("speaker")} className="w-full" id="settings-speaker">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value={DEFAULT_DEVICE_VALUE}>{t("speakerDefault")}</SelectItem>
                        {speakers.map((device) => (
                          <SelectItem key={device.deviceId} value={device.deviceId}>
                            {device.label || t("speaker")}
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </FieldRow>
                </SettingsSection>
              </TabsContent>

              <TabsContent value="advanced">
                <SettingsSection
                  description={t("browserProcessingDescription")}
                  title={t("browserProcessing")}
                >
                  <SwitchRow
                    checked={audioDiagnosticsEnabled}
                    description={t("audioDiagnosticsDescription")}
                    id="settings-audio-diagnostics"
                    label={t("audioDiagnostics")}
                    onCheckedChange={setAudioDiagnosticsEnabled}
                  />
                  <SwitchRow
                    checked={audioProcessing.echoCancellation}
                    description={t("echoCancellationDescription")}
                    id="settings-echo-cancellation"
                    label={t("echoCancellation")}
                    onCheckedChange={(checked) =>
                      setAudioProcessing({
                        ...audioProcessing,
                        echoCancellation: checked
                      })
                    }
                  />
                  <SwitchRow
                    checked={audioProcessing.autoGainControl}
                    description={t("autoGainControlDescription")}
                    id="settings-auto-gain-control"
                    label={t("autoGainControl")}
                    onCheckedChange={(checked) =>
                      setAudioProcessing({
                        ...audioProcessing,
                        autoGainControl: checked
                      })
                    }
                  />
                  <SwitchRow
                    checked={audioProcessing.noiseSuppression}
                    description={t("browserNoiseSuppressionDescription")}
                    id="settings-browser-noise"
                    label={t("browserNoiseSuppression")}
                    onCheckedChange={(checked) =>
                      setAudioProcessing({
                        ...audioProcessing,
                        noiseSuppression: checked
                      })
                    }
                  />
                </SettingsSection>
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

function SettingsSection({
  children,
  description,
  title
}: {
  children: React.ReactNode;
  description: string;
  title: string;
}) {
  return (
    <section className="grid gap-4">
      <div>
        <h2 className="text-sm font-semibold text-foreground">{title}</h2>
        <p className="mt-1 text-sm text-lyre-muted-foreground">{description}</p>
      </div>
      <div className="grid gap-3">{children}</div>
    </section>
  );
}

function FieldRow({
  children,
  htmlFor,
  label
}: {
  children: React.ReactNode;
  htmlFor: string;
  label: string;
}) {
  return (
    <div className="grid gap-2 sm:grid-cols-[9rem_minmax(0,1fr)] sm:items-center">
      <label className="text-sm font-medium text-lyre-soft-foreground sm:text-right" htmlFor={htmlFor}>
        {label}
      </label>
      {children}
    </div>
  );
}

function SwitchRow({
  checked,
  description,
  id,
  label,
  onCheckedChange
}: {
  checked: boolean;
  description: string;
  id: string;
  label: string;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <div className="flex items-center justify-between gap-4 rounded-lg border border-lyre-subtle-border px-3 py-3">
      <label className="min-w-0 text-sm" htmlFor={id}>
        <span className="font-medium text-foreground">{label}</span>
        <span className="mt-0.5 block text-lyre-muted-foreground">{description}</span>
      </label>
      <Switch
        aria-label={label}
        checked={checked}
        id={id}
        onCheckedChange={onCheckedChange}
      />
    </div>
  );
}
