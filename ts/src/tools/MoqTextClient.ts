import * as Moq from "@kixelated/moq"
import { ShareFile } from "./ShareFile";

enum ClientType {
  Publisher,
  Subscriber,
}

export class MoqTextClient {
  private connection : Moq.Connection.Established | null = null;
  private broadcast : Moq.Broadcast | null = null;
  private namespace : string | null = null;
  private files : Map<string,ShareFile> = new Map();
  private clientType : ClientType | null = null;

  private _textReceiveCallback: ((track:string, text:string) => void) | null = null;

  set textReceiveCallback(callback: (track:string, text:string) => void) {
    this._textReceiveCallback = callback;
  }

  async startPublish(url : string, ns: string, track : string){
    if (!await this.connect(url, ns))
      return
    this.clientType = ClientType.Publisher;
    this._startPub(track)
  }
  async startSubscribe(url : string, ns: string, track : string){
    if (!await this.connect(url, ns))
      return
    this.clientType = ClientType.Subscriber;
    this._startSub(track)
  }

  publish(trackName:string, text:string) {
    const sf = this.files.get(trackName);
    if (sf) {
      console.log("published on ["+trackName+"] : ["+text+"]")
      sf.track.writeString(text);
    }
  }

  disconnect() {
    if (this.clientType == ClientType.Publisher){
      this.connection?.close();
    }
    this.broadcast?.close();
    this.files.clear();
  }

  isConnected() : boolean {
    return this.connection !== null;
  }

  private async connect(url : string, ns: string) {
    try {
      this.connection = await Moq.Connection.connect(new URL(url));
      this.namespace = ns;
      return true
    }catch(e){
      console.error(e)
      return false
    }
  }

  private async _startPub(trackname : string){
    if (!this.connection || !this.namespace)
      return

    this.broadcast = new Moq.Broadcast();
    this.connection.publish(Moq.Path.from(this.namespace), this.broadcast);

    for (;;) {
      const request = await this.broadcast.requested();
      if (!request) break;

      if (request.track.name === trackname) {
        this.files.set(trackname, new ShareFile(trackname, request.track))
        console.log("inserted request track")
      } else {
        request.track.close(new Error("not found"));
      }
    }
  }

  private async _startSub(trackname : string){
    if (!this.connection || !this.namespace)
      return
    const broadcast = this.connection.consume(Moq.Path.from(this.namespace));
    const track = broadcast.subscribe(trackname, 0);
    this.files.set(trackname, new ShareFile(trackname, track))
    console.log("sub connection done on track"+trackname);

  }

  private async _keepAliveSub(){
    for (;;) {
      this.files.forEach(async (val,key,m)=>{
        const group = await val.syncTrack.nextGroup();
        if (!group) {
          console.log("No next group");
          return;
        }

        for (;;) {
          const frame = await group.readString();
          if (frame === undefined) { // currently group.readString() returns string | undefined.
            break; // End of group
          }
          if (this._textReceiveCallback) {
              console.log("frame",frame)
              this._textReceiveCallback(val.trackName, frame);
          }
        }
      })
    }
  }

}
