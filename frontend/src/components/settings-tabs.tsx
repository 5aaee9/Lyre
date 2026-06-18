"use client";

import { useTranslations } from "next-intl";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import {
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { parseNoiseProvider } from "@/lib/api";
import {
  supportedLanguages,
  type AudioDeviceConfig,
  type AudioProcessingConfig,
  type SettingsSnapshot
} from "@/lib/settings-store";

type ProfileSettingsProps = {
  language: SettingsSnapshot["language"];
  nickname: string;
  setLanguage: (language: SettingsSnapshot["language"]) => void;
  setNickname: (nickname: string) => void;
};

type NoiseSettingsProps = {
  noise: SettingsSnapshot["noise"];
  setNoise: (noise: SettingsSnapshot["noise"]) => void;
};

type DeviceSettingsProps = {
  audioDevices: AudioDeviceConfig;
  microphones: MediaDeviceInfo[];
  setAudioDevices: (audioDevices: AudioDeviceConfig) => void;
  speakers: MediaDeviceInfo[];
};

type AdvancedSettingsProps = {
  audioDiagnosticsEnabled: boolean;
  audioProcessing: AudioProcessingConfig;
  setAudioDiagnosticsEnabled: (enabled: boolean) => void;
  setAudioProcessing: (audioProcessing: AudioProcessingConfig) => void;
};

const DEFAULT_DEVICE_VALUE = "default";

export function ProfileSettings({
  language,
  nickname,
  setLanguage,
  setNickname
}: ProfileSettingsProps) {
  const t = useTranslations("Settings");
  return (
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
  );
}

export function NoiseSettings({ noise, setNoise }: NoiseSettingsProps) {
  const t = useTranslations("Settings");
  return (
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
  );
}

export function DeviceSettings({
  audioDevices,
  microphones,
  setAudioDevices,
  speakers
}: DeviceSettingsProps) {
  const t = useTranslations("Settings");
  return (
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
  );
}

export function AdvancedSettings({
  audioDiagnosticsEnabled,
  audioProcessing,
  setAudioDiagnosticsEnabled,
  setAudioProcessing
}: AdvancedSettingsProps) {
  const t = useTranslations("Settings");
  return (
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
      <SwitchRow
        checked={audioProcessing.clientNoiseCancellation}
        description={t("clientNoiseCancellationDescription")}
        id="settings-client-noise"
        label={t("clientNoiseCancellation")}
        onCheckedChange={(checked) =>
          setAudioProcessing({
            ...audioProcessing,
            clientNoiseCancellation: checked
          })
        }
      />
    </SettingsSection>
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
