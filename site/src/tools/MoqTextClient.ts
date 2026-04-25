import * as Moq from "@kixelated/moq"

class SyncMessage {
  constructor(public client: string, public version: number, public ops: unknown) {}
  static parse(text: string): SyncMessage | null {
    const parts = text.split(';');
    if (parts.length < 3) return null;
    const client = parts[0];
    const vStr = parts[1];
    if (client === undefined || vStr === undefined) return null;
    const version = parseInt(vStr, 10);
    if (isNaN(version)) return null;
    try {
      const ops = JSON.parse(parts.slice(2).join(';'));
      return new SyncMessage(client, version, ops);
    } catch {
      return null;
    }
  }
}

class StateMessage {
  constructor(public version: number, public text: string) {}
  static parse(text: string): StateMessage | null {
    const splitIdx = text.indexOf(';');
    if (splitIdx === -1) return null;
    const version = parseInt(text.substring(0, splitIdx), 10);
    if (isNaN(version)) return null;
    const body = text.substring(splitIdx + 1);
    return new StateMessage(version, body);
  }
}

export class MoqTextClient {
  private connection: Moq.Connection.Established | null = null;
  private clientName: string;
  private bc_name : string | null = null;
  private rcv_callback : ((t:string)=>void) | null = null;

  private update_track : Moq.Track | null = null;
  private handledPaths : Set<string> = new Set();
  private connecting : boolean = false;
  private lastKnownText : string = "";
  private lastKnownVersion : number = -1;

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

  }

  private async handleSyncBc(path: Moq.Path.Valid) {
    if (!this.connection) return;

    const pathStr = path.toString();
    const bc = this.connection.consume(path);
    const sync_track = bc.subscribe("sync", 0);
    const state_track = bc.subscribe("state", 0);
    console.log(`Subscribed to track "update" on broadcast: [${pathStr}]`);

    // Read incoming messages
    this.readSyncTrack(sync_track, pathStr)
    this.readStateTrack(state_track, pathStr)
  }
  private async readSyncTrack(track: Moq.Track, pathStr: string) {
    for (;;) {
      if (!this.connection) {
        break
      }
      const text = await track.readString();
      if (text === undefined) {
        console.log(`Track "sync" closed for [${pathStr}]`);
        break;
      }

      const msg = SyncMessage.parse(text);
      if (!msg) {
        console.log(`Failed to parse sync msg: ${text}`);
        continue;
      }

      if (msg.version <= this.lastKnownVersion) {
        console.log(`Ignoring stale sync update: ${msg.version} <= ${this.lastKnownVersion}`);
        continue;
      }

      this.lastKnownVersion = msg.version;
      if (msg.client == this.clientName) {
        continue
      }

      if (Array.isArray(msg.ops)) {
        console.log(`GOT OT [${pathStr}/${track.name}] (v${msg.version}): ${msg.ops}`);
        this.lastKnownText = this.applyOT(this.lastKnownText, msg.ops as (string | number)[]);
      } else if (typeof msg.ops === "string") {
        console.log(`GOT full text [${pathStr}/] (v${msg.version}): ${msg.ops}`);
        this.lastKnownText = msg.ops;
      }

      if (this.rcv_callback) {
        console.log("got callback: ", this.lastKnownText)
        this.rcv_callback(this.lastKnownText);
      }
    }
  }

  private async readStateTrack(track: Moq.Track, pathStr: string) {
    for (;;) {
      if (!this.connection) {
        break
      }
      const text = await track.readString();
      if (text === undefined) {
        console.log(`Track "state" closed for [${pathStr}]`);
        break;
      }

      const msg = StateMessage.parse(text);
      if (!msg) {
        console.log(`Failed to parse state msg: ${text}`);
        continue;
      }

      if (msg.version <= this.lastKnownVersion) {
        console.log(`Ignoring stale state update: ${msg.version} <= ${this.lastKnownVersion}`);
        continue;
      }

      console.log(`STATE RECALL (v${msg.version}): ${msg.text}`);
      this.lastKnownText = msg.text
      this.lastKnownVersion = msg.version

      if (this.rcv_callback) {
        this.rcv_callback(this.lastKnownText);
      }
    }
  }
  private async sleep(ms: number): Promise<void> {
      return new Promise(
          (resolve) => setTimeout(resolve, ms)
      );
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
