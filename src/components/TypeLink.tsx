import { useNavigate } from 'react-router-dom';
import { getTypeById } from '../data/types';

interface TypeLinkProps {
  typeId: string;
  className?: string;
}

export default function TypeLink({ typeId, className = '' }: TypeLinkProps) {
  const navigate = useNavigate();
  const dt = getTypeById(typeId);

  if (!dt) {
    return <span className={`font-mono text-slate-300 ${className}`}>{typeId}</span>;
  }

  return (
    <button
      onClick={() => navigate(`/type/${typeId}`)}
      className={`font-mono text-sky-400 hover:text-sky-300 hover:underline underline-offset-2 transition-colors cursor-pointer ${className}`}
    >
      {dt.name}
    </button>
  );
}

// Parse a type string and make type references clickable
export function TypeString({ text, className = '' }: { text: string; className?: string }) {
  const navigate = useNavigate();
  // Known type IDs to look for
  const knownTypes = [
    'SigningPayload', 'SigningResult', 'SigningErr', 'SigningRawPayload', 'RawPayload',
    'ProductAccountId', 'Account', 'ContextualAlias', 'RingLocation', 'RingLocationHint',
    'RingVrfProof', 'RequestCredentialsErr', 'CreateProofErr', 'AccountConnectionStatus',
    'VersionedTxPayload', 'TxPayloadV1', 'TxPayloadContextV1', 'TxPayloadExtensionV1',
    'CreateTransactionErr', 'Feature', 'NavigateToErr', 'PushNotification',
    'DevicePermissionRequest', 'RemotePermissionRequest', 'GenericError', 'GenericErr',
    'StorageKey', 'StorageValue', 'StorageErr', 'GenesisHash',
    'ChatRoomRequest', 'ChatRoomRegistrationResult', 'ChatRoomRegistrationErr',
    'ChatBotRequest', 'ChatBotRegistrationResult', 'ChatBotRegistrationErr',
    'ChatMessageContent', 'ChatPostMessageResult', 'ChatMessagePostingErr',
    'ChatRoom', 'ReceivedChatAction', 'ChatActionPayload', 'CustomRendererNode',
    'ChainHeadEvent', 'OperationStartedResult', 'StorageQueryItem', 'StorageQueryType',
    'SignedStatement', 'StatementProof', 'StatementProofErr', 'Statement',
    'PreimageKey', 'PreimageValue', 'PreimageSubmitErr',
    'BlockHash', 'OperationId', 'RuntimeSpec', 'RuntimeType',
  ];

  // Sort by length (longest first) to avoid partial matches
  const sorted = [...knownTypes].sort((a, b) => b.length - a.length);

  const parts: { text: string; isType: boolean; typeId: string }[] = [];
  let remaining = text;

  while (remaining.length > 0) {
    let earliestIndex = Infinity;
    let earliestType = '';

    for (const typeName of sorted) {
      const idx = remaining.indexOf(typeName);
      if (idx !== -1 && idx < earliestIndex) {
        earliestIndex = idx;
        earliestType = typeName;
      }
    }

    if (earliestType && earliestIndex < Infinity) {
      if (earliestIndex > 0) {
        parts.push({ text: remaining.slice(0, earliestIndex), isType: false, typeId: '' });
      }
      parts.push({ text: earliestType, isType: true, typeId: earliestType });
      remaining = remaining.slice(earliestIndex + earliestType.length);
    } else {
      parts.push({ text: remaining, isType: false, typeId: '' });
      break;
    }
  }

  return (
    <span className={`font-mono text-sm ${className}`}>
      {parts.map((part, i) =>
        part.isType ? (
          <button
            key={i}
            onClick={() => navigate(`/type/${part.typeId}`)}
            className="text-sky-400 hover:text-sky-300 hover:underline underline-offset-2 transition-colors cursor-pointer"
          >
            {part.text}
          </button>
        ) : (
          <span key={i} className="text-slate-300">{part.text}</span>
        )
      )}
    </span>
  );
}
