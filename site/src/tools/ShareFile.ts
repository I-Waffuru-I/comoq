import type { Track } from "@kixelated/moq";

export class ShareFile {
  trackName : string;
  syncTrack : Track;
  pushTracks : Track[];

  constructor(trackName : string, track : Track){
    this.trackName = trackName;
    this.syncTrack = track;
    this.pushTracks = [];
  }

  addPushingTrack(track:Track){
    this.pushTracks.push(track)
  }
}
