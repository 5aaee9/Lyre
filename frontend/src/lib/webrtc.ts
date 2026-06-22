import type { IceServerConfig } from "./api";
import { processLocalAudioStream } from "./client-noise-cancellation";
import {
  readAudioDeviceConfig,
  readAudioProcessingConfig,
  readNoiseConfig,
  writeAudioDeviceConfig
} from "./storage";
import type { AudioProcessingConfig } from "./settings-store";

export async function openLocalAudioStream(): Promise<MediaStream> {
  const audioProcessing = readAudioProcessingConfig();
  const audioDevices = readAudioDeviceConfig();
  const noise = readNoiseConfig();
  let stream: MediaStream;
  try {
    stream = await navigator.mediaDevices.getUserMedia(localAudioConstraints(audioProcessing, audioDevices.inputDeviceId));
  } catch (error) {
    if (!audioDevices.inputDeviceId || !isMissingAudioInputError(error)) {
      throw error;
    }
    writeAudioDeviceConfig({ ...audioDevices, inputDeviceId: "" });
    stream = await navigator.mediaDevices.getUserMedia(localAudioConstraints(audioProcessing, ""));
  }
  if (!audioProcessing.clientNoiseCancellation) {
    return stream;
  }
  try {
    return await processLocalAudioStream(stream, { noise });
  } catch {
    return stream;
  }
}

function localAudioConstraints(audioProcessing: AudioProcessingConfig, inputDeviceId: string): MediaStreamConstraints {
  return {
    audio: {
      ...(inputDeviceId ? { deviceId: { exact: inputDeviceId } } : {}),
      echoCancellation: audioConstraint(audioProcessing.echoCancellation),
      autoGainControl: audioConstraint(audioProcessing.autoGainControl),
      noiseSuppression: audioConstraint(audioProcessing.noiseSuppression)
    }
  };
}

export function isMissingAudioInputError(error: unknown): boolean {
  if (!(error instanceof Error)) {
    return false;
  }
  return error.name === "NotFoundError" || (
    error.name === "OverconstrainedError" &&
    "constraint" in error &&
    error.constraint === "deviceId"
  );
}

function audioConstraint(enabled: boolean): boolean | ConstrainBooleanParameters {
  return enabled ? true : { exact: false };
}

export type PeerConnectionOptions = {
  receiveOnlyAudio?: boolean;
};

export function createPeerConnection(
  iceServers: IceServerConfig[],
  stream: MediaStream,
  options: PeerConnectionOptions = {}
): RTCPeerConnection {
  const connection = new RTCPeerConnection({
    iceServers: iceServers.map((server) => ({
      urls: server.urls,
      username: server.username ?? undefined,
      credential: server.credential ?? undefined
    }))
  });
  const audioTracks = stream.getAudioTracks();
  for (const track of audioTracks) {
    connection.addTrack(track, stream);
  }
  if (options.receiveOnlyAudio && audioTracks.length === 0) {
    connection.addTransceiver("audio", { direction: "recvonly" });
  }
  return connection;
}

export async function createAudioPeerConnection(iceServers: IceServerConfig[]): Promise<RTCPeerConnection> {
  return createPeerConnection(iceServers, await openLocalAudioStream());
}
