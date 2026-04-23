import * as Moq from "@kixelated/moq"

export class MoqTextClient {
  private connection: Moq.Connection.Established | null = null;
  private clientName: string;
  private bc_name : string | null = null;
  private rcv_callback : ((t:string)=>void) | null = null;

  private update_track : Moq.Track | null = null;
  private handledPaths : Set<string> = new Set();
  private connecting : boolean = false;
  private lastKnownText : string = "";

  constructor(clientName: string = "client_" + Math.floor(Math.random() * 1000)) {
    this.clientName = clientName;
  }

  set_callback(callback : ((t : string)=>void)){
    this.rcv_callback = callback;
  }

  async run(url: string) {
    if (this.connecting) return;
    this.connecting = true;
    this.disconnect();
    try {
      this.connection = await Moq.Connection.connect(new URL(url));
      console.log("Connected to", url);
      this.handleAnnounced()
    } catch (e) {
      console.error("Connection failed", e);
    } finally {
      this.connecting = false;
    }
  }

  public async update(text : string) {
    if (this.update_track) {
      const ot = this.computeOT(this.lastKnownText, text);
      const jsonOt = JSON.stringify(ot);
      console.log(`Sending OT: ${jsonOt}`);
      this.update_track.writeString(jsonOt);
      this.lastKnownText = text;
    }
  }

  private computeOT(oldText: string, newText: string): (string | number)[] {
    let prefixLen = 0;
    while (prefixLen < oldText.length && prefixLen < newText.length && oldText[prefixLen] === newText[prefixLen]) {
      prefixLen++;
    }

    let suffixLen = 0;
    while (
      suffixLen < oldText.length - prefixLen &&
      suffixLen < newText.length - prefixLen &&
      oldText[oldText.length - 1 - suffixLen] === newText[newText.length - 1 - suffixLen]
    ) {
      suffixLen++;
    }

    const ops: (string | number)[] = [];
    if (prefixLen > 0) {
      ops.push(prefixLen);
    }

    const deleteLen = oldText.length - prefixLen - suffixLen;
    const insertText = newText.substring(prefixLen, newText.length - suffixLen);

    if (insertText.length > 0) {
      ops.push(insertText);
    }
    if (deleteLen > 0) {
      ops.push(-deleteLen);
    }

    if (suffixLen > 0) {
      ops.push(suffixLen);
    }

    return ops;
  }

  private async handleAnnounced() {
    if (!this.connection) return;

    const entry = await this.connection.announced().next();

    if (!entry || !entry.path.includes("file1")) {
      return
    }

    const path_str = entry.path.toString();
    // found file broadcast
    console.log("HANDLE BC: ", path_str);
    this.bc_name = path_str
    this.handledPaths.add(path_str);

    // handle incoming updates
    this.handleSyncBc(entry.path);
    // start own bc
    this.handleOwnBroadcast(path_str)

    // for (;;) {
    //   const entry = await this.connection.announced().next();
    //   if (!entry) break;
    //
    //   const path_str = entry.path.toString();
    //
    //   if (!entry.active) {
    //     console.log("Broadcast unannounced:", path_str);
    //     this.handledPaths.delete(path_str);
    //     continue;
    //   }
    //
    //   if (this.handledPaths.has(path_str)) {
    //     continue;
    //   }
    //
    //   if (path_str.includes("/client/")) {
    //     continue;
    //   }
    //
    //   // found file broadcast
    //   console.log("HANDLE BC: ", path_str);
    //   this.bc_name = path_str
    //   this.handledPaths.add(path_str);
    //
    //   // handle incoming updates
    //   this.handleSyncBc(entry.path);
    //   // start own bc
    //   this.handleOwnBroadcast(path_str)
    // }
  }

  private async handleSyncBc(path: Moq.Path.Valid) {
    if (!this.connection) return;

    const pathStr = path.toString();
    const bc = this.connection.consume(path);
    const track = bc.subscribe("sync", 0);
    console.log(`Subscribed to track "update" on broadcast: [${pathStr}]`);

    // Read incoming messages
    this.readTrack(track, pathStr);
  }
  private async readTrack(track: Moq.Track, pathStr: string) {
    for (;;) {
      const text = await track.readString();
      if (text === undefined) {
        console.log(`Track "update" closed for [${pathStr}]`);
        break;
      }

      try {
        const ops = JSON.parse(text);
        if (Array.isArray(ops)) {
          console.log(`GOT OT [${pathStr}/sync]: ${text}`);
          this.lastKnownText = this.applyOT(this.lastKnownText, ops);
        } else {
          console.log(`GOT full text [${pathStr}/sync]: ${text}`);
          this.lastKnownText = text;
        }
      } catch {
        // Fallback for non-JSON or weird strings
        console.log(`GOT raw [${pathStr}/sync]: ${text}`);
        this.lastKnownText = text;
      }

      if (this.rcv_callback) {
        console.log("got callback: ", this.lastKnownText)
        this.rcv_callback(this.lastKnownText);
      }
    }
  }

  private applyOT(text: string, ops: (string | number)[]): string {
    let result = "";
    let cursor = 0;
    for (const op of ops) {
      if (typeof op === "string") {
        // Insert
        result += op;
      } else if (op > 0) {
        // Retain
        result += text.substring(cursor, cursor + op);
        cursor += op;
      } else {
        // Delete
        cursor += Math.abs(op);
      }
    }
    return result;
  }

  private async handleOwnBroadcast(pathStr : string) {
    if (!this.connection){
      return
    }
    // Start its own broadcast `file_name/client/client_name`
    const ownBcPathStr = `${pathStr}/client/${this.clientName}`;
    const ownBcPath = Moq.Path.from(ownBcPathStr);

    const bc = new Moq.Broadcast();
    this.connection.publish(ownBcPath, bc);
    console.log(`Published own broadcast: [${ownBcPathStr}]`);

    for (;;) {
      const request = await bc.requested();
      if (!request) break;

      if (request.track.name === "update") {
        console.log(`req for "update" track`);
        this.update_track = request.track;
      } else {
        request.track.close(new Error("not found"));
      }
    }
  }

  disconnect() {
    this.connection?.close();
    this.connection = null
    this.update_track = null
    this.bc_name = null
    this.handledPaths.clear();
  }

  isConnected(): boolean {
    return this.connection !== null;
  }
}
