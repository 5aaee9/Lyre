export async function createAudioPeerConnection(): Promise<RTCPeerConnection> {
  const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  const connection = new RTCPeerConnection();
  for (const track of stream.getAudioTracks()) {
    connection.addTrack(track, stream);
  }
  return connection;
}
