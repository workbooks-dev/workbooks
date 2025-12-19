export function FileInfoDialog({ fileInfo, onClose }) {
  if (!fileInfo) return null;

  const formatBytes = (bytes) => {
    if (bytes === 0) return '0 Bytes';
    const k = 1024;
    const sizes = ['Bytes', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
  };

  const formatDate = (timestamp) => {
    if (!timestamp) return 'N/A';
    const date = new Date(parseInt(timestamp) * 1000);
    return date.toLocaleString();
  };

  return (
    <div
      className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-lg shadow-xl p-6 w-full max-w-md"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-lg font-semibold text-gray-900 mb-4">File Information</h2>

        <div className="space-y-3 text-sm">
          <div>
            <div className="text-xs text-gray-500 uppercase tracking-wider mb-1">Name</div>
            <div className="text-gray-900 font-medium">{fileInfo.name}</div>
          </div>

          <div>
            <div className="text-xs text-gray-500 uppercase tracking-wider mb-1">Path</div>
            <div className="text-gray-700 text-xs break-all font-mono bg-gray-50 p-2 rounded">
              {fileInfo.path}
            </div>
          </div>

          <div>
            <div className="text-xs text-gray-500 uppercase tracking-wider mb-1">Type</div>
            <div className="text-gray-900">
              {fileInfo.is_dir ? 'Folder' : 'File'}
            </div>
          </div>

          {fileInfo.is_file && (
            <div>
              <div className="text-xs text-gray-500 uppercase tracking-wider mb-1">Size</div>
              <div className="text-gray-900">{formatBytes(fileInfo.size)}</div>
            </div>
          )}

          <div>
            <div className="text-xs text-gray-500 uppercase tracking-wider mb-1">Modified</div>
            <div className="text-gray-900">{formatDate(fileInfo.modified)}</div>
          </div>

          <div>
            <div className="text-xs text-gray-500 uppercase tracking-wider mb-1">Created</div>
            <div className="text-gray-900">{formatDate(fileInfo.created)}</div>
          </div>

          <div>
            <div className="text-xs text-gray-500 uppercase tracking-wider mb-1">Permissions</div>
            <div className="text-gray-900">
              {fileInfo.readonly ? 'Read-only' : 'Read & Write'}
            </div>
          </div>
        </div>

        <div className="mt-6 flex justify-end">
          <button
            onClick={onClose}
            className="px-4 py-2 bg-blue-600 text-white text-sm font-medium rounded hover:bg-blue-700 transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
