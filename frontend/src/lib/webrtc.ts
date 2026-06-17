import type { IceServerConfig } from "./api";
import { readAudioDeviceConfig, readAudioProcessingConfig } from "./storage";

export async function openLocalAudioStream(): Promise<MediaStream> {
  const audioProcessing = readAudioProcessingConfig();
  const audioDevices = readAudioDeviceConfig();
  return navigator.mediaDevices.getUserMedia({
    audio: {
      ...(audioDevices.inputDeviceId ? { deviceId: { exact: audioDevices.inputDeviceId } } : {}),
      echoCancellation: audioConstraint(audioProcessing.echoCancellation),
      autoGainControl: audioConstraint(audioProcessing.autoGainControl),
      noiseSuppression: audioConstraint(audioProcessing.noiseSuppression)
    }
  });
}

function audioConstraint(enabled: boolean): boolean | ConstrainBooleanParameters {
  return enabled ? true : { exact: false };
}

export function createPeerConnection(iceServers: IceServerConfig[], stream: MediaStream): RTCPeerConnection {
  const connection = new RTCPeerConnection({
    iceServers: iceServers.map((server) => ({
      urls: server.urls,
      username: server.username ?? undefined,
      credential: server.credential ?? undefined
    }))
  });
  for (const track of stream.getAudioTracks()) {
    connection.addTrack(track, stream);
  }
  return connection;
}

export async function createAudioPeerConnection(iceServers: IceServerConfig[]): Promise<RTCPeerConnection> {
  return createPeerConnection(iceServers, await openLocalAudioStream());
}
