import type { Track } from '../types'
import { fmtTime, displayTitle, audioQuality } from '../util'

interface Props {
  track: Track
  onClose: () => void
}

function Row({ label, value }: { label: string; value: string | null }) {
  if (value === null || value === '') return null
  return (
    <div className="fileInfoRow">
      <span className="fileInfoLabel">{label}</span>
      <span className="fileInfoValue">{value}</span>
    </div>
  )
}

export function FileInfo({ track, onClose }: Props) {
  const mtime = track.file_mtime
    ? new Date(track.file_mtime * 1000).toLocaleString()
    : null

  return (
    <div className="trackMenuModal" onClick={onClose}>
      <div className="fileInfoBox" onClick={(e) => e.stopPropagation()}>
        <div className="fileInfoHead">
          <span className="fileInfoTitle">{displayTitle(track)}</span>
          <button className="trackMenuClose" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>

        <div className="fileInfoSection">Location</div>
        <Row label="File" value={track.path} />
        <Row label="URI" value={track.uri} />
        <Row label="Modified" value={mtime} />

        <div className="fileInfoSection">Audio</div>
        <Row label="Format" value={track.format} />
        {track.format || track.sample_rate ? (
          <Row label="Quality" value={audioQuality(track)} />
        ) : null}
        <Row
          label="Sample rate"
          value={track.sample_rate ? `${track.sample_rate} Hz` : null}
        />
        <Row
          label="Bit depth"
          value={track.bit_depth ? `${track.bit_depth}-bit` : null}
        />
        <Row
          label="Channels"
          value={track.channels ? String(track.channels) : null}
        />
        <Row label="Duration" value={fmtTime(track.duration)} />
        {track.cue_index !== null ? (
          <Row label="CUE track" value={String(track.cue_index)} />
        ) : null}

        <div className="fileInfoSection">Tags</div>
        <Row label="Title" value={track.title} />
        <Row label="Artist" value={track.artist} />
        <Row label="Album" value={track.album} />
        <Row label="Genre" value={track.genre} />
        <Row label="Year" value={track.year ? String(track.year) : null} />
        <Row label="Track" value={track.track ? String(track.track) : null} />
      </div>
    </div>
  )
}
