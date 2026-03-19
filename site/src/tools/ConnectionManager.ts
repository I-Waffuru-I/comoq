import { MoqTextClient } from "./MoqTextClient";


class ConnectionManager {
  client: MoqTextClient
  constructor() {
    this.client = new MoqTextClient()
  }

  async run(url: string) {
    this.client.run(url)
  }

  set_callback(callback : ((t:string)=>void)){
    this.client.set_callback(callback)
  }

  update(text : string){
    this.client.update(text)
  }

  disconnect() {
    this.client.disconnect();
  }

  isConnected(): boolean {
    return this.client.isConnected()
  }
}

export const connectionManager = new ConnectionManager();




