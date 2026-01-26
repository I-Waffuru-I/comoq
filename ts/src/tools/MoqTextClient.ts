import * as Moq from "@kixelated/moq"

enum ClientType {
  Publisher,
  Subscriber,
}

export class MoqTextClient {
  private connection : Moq.Connection.Established | null = null;
  private broadcast : Moq.Broadcast | null = null;
  private track : Moq.Track | null = null;
  private namespace : string | null = null;
  private trackName : string | null = null;
  private clientType : ClientType | null = null;

  private _textReceiveCallback: ((text: string) => void) | null = null;

  set textReceiveCallback(callback: (text: string) => void) {
    this._textReceiveCallback = callback;
  }

  async startPublish(url : string, ns: string, track : string){
    if (!await this.connect(url, ns, track))
      return
    this._startPub()
    this.clientType = ClientType.Publisher;
    //this._startSub()
  }
  async startSubscribe(url : string, ns: string, track : string){
    if (!await this.connect(url, ns, track))
      return
    this._startSub()
    this.clientType = ClientType.Subscriber;
  }

  publish(text: string) {
    if (this.track) {
      this.track.writeString(text);
    }
  }

  disconnect() {
    if (this.clientType == ClientType.Publisher){
      this.connection?.close();
    }
    this.track = null
    this.connection = null
  }

  isConnected() {
    return this.connection !== null;
  }

  private async connect(url : string, ns: string, track : string) {
    try {
      this.connection = await Moq.Connection.connect(new URL(url));
      this.trackName = track;
      this.namespace = ns;
      return true
    }catch(e){
      return false
    }
  }

  private async _startPub(){
    if (!this.connection || !this.namespace || !this.trackName)
      return

    this.broadcast = new Moq.Broadcast();
    this.connection.publish(Moq.Path.from(this.namespace), this.broadcast);

    for (;;) {
      const request = await this.broadcast.requested();
      if (!request) break;

      if (request.track.name === this.trackName) {
        this.track = request.track
        console.log("got request track")
        return true
      } else {
        request.track.close(new Error("not found"));
      }
    }
  }

  private async _startSub(){
    if (!this.connection || !this.trackName || !this.namespace)
      return
    const broadcast = this.connection.consume(Moq.Path.from(this.namespace));
    const track = broadcast.subscribe(this.trackName, 0);
    console.log("sub connection done");

    for (;;) {
      const group = await track.nextGroup();
      if (!group) {
        console.log("No next group");
        break;
      }

      for (;;) {
        const frame = await group.readString();
        if (frame === undefined) { // currently group.readString() returns string | undefined.
          break; // End of group
        }
        if (this._textReceiveCallback) {
            console.log("frame",frame)
            this._textReceiveCallback(frame);
        }
      }
    }
  }

}
