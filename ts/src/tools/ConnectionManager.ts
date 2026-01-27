import { MoqTextClient } from "./MoqTextClient";


class ConnectionManager  {
  client : MoqTextClient
  constructor() {
    this.client = new MoqTextClient()
  }

  async startPublish(url : string, ns: string, track : string) {
    this.client.startPublish(url, ns, track)
  }
  async startSubscribe(url : string, ns: string, track : string) {
    this.client.startSubscribe(url, ns, track)
  }

  send(text: string) {
    this.client.publish(text);
  }

  onMessage(callback: (text: string) => void) {
    this.client.textReceiveCallback = callback;
  }

  disconnect() {
    this.client.disconnect();
  }

  isConnected() : boolean {
    return this.client.isConnected()
  }
}

export const connectionManager = new ConnectionManager();




