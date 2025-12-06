
export class ConnectionManager {
   _connected : boolean;
   _path : string | null;

   constructor() {
      this._connected = false;  
      this._path = null;
   }

   isConnected() : boolean {
      return this._connected 
   }
   
   async tryConnect(path : string) : Promise<boolean> {
      this._path = path;
      this._connected = true;
      return true
   }

};

export const connectionManager = new ConnectionManager();




