/**
 * AMOS Wallet Connection Module
 * Handles Solana wallet connection, signing, and backend registration.
 * Dependencies: @solana/web3.js (loaded via CDN in index.html)
 */

const AMOSWallet = {
    connected: false,
    publicKey: null,
    provider: null,
    walletName: null,
    balance: null,
    _listeners: [],

    // ========================================================================
    // Wallet Detection
    // ========================================================================

    /**
     * Detect available Solana wallet providers in the browser.
     * Returns an array of { name, icon, provider } objects.
     */
    getAvailableWallets: function() {
        var wallets = [];

        // Phantom
        if (window.phantom && window.phantom.solana && window.phantom.solana.isPhantom) {
            wallets.push({
                name: 'phantom',
                displayName: 'Phantom',
                icon: '/static/img/phantom-icon.svg',
                provider: window.phantom.solana,
            });
        }

        // Solflare
        if (window.solflare && window.solflare.isSolflare) {
            wallets.push({
                name: 'solflare',
                displayName: 'Solflare',
                icon: '/static/img/solflare-icon.svg',
                provider: window.solflare,
            });
        }

        return wallets;
    },

    // ========================================================================
    // Connect / Disconnect
    // ========================================================================

    /**
     * Connect to a wallet by provider name ('phantom' or 'solflare').
     * Signs a verification message and registers with the backend.
     */
    connect: async function(providerName) {
        var wallets = this.getAvailableWallets();
        var wallet = wallets.find(function(w) { return w.name === providerName; });
        if (!wallet) {
            throw new Error('Wallet provider "' + providerName + '" not found. Is the extension installed?');
        }

        try {
            // Request connection
            var resp = await wallet.provider.connect();
            var pubkey = resp.publicKey || wallet.provider.publicKey;
            if (!pubkey) {
                throw new Error('No public key returned from wallet');
            }

            this.provider = wallet.provider;
            this.walletName = wallet.name;
            this.publicKey = pubkey.toString();
            this.connected = true;

            // Listen for disconnect events
            this._setupListeners();

            // Sign verification message and register with backend
            await this.registerWallet();

            // Persist connection preference
            localStorage.setItem('amos-wallet-provider', providerName);

            // Fetch token balance
            await this.refreshBalance();

            // Update all UI elements
            this.updateUI();

            return this.publicKey;
        } catch (err) {
            this.connected = false;
            this.publicKey = null;
            this.provider = null;
            this.walletName = null;
            throw err;
        }
    },

    /**
     * Disconnect from the current wallet and clean up.
     */
    disconnect: async function() {
        if (this.provider) {
            try {
                await this.provider.disconnect();
            } catch (e) {
                console.warn('Wallet disconnect error:', e);
            }
        }

        this._removeListeners();
        this.connected = false;
        this.publicKey = null;
        this.provider = null;
        this.walletName = null;
        this.balance = null;

        localStorage.removeItem('amos-wallet-provider');

        this.updateUI();
    },

    // ========================================================================
    // Signing
    // ========================================================================

    /**
     * Sign an arbitrary message with the connected wallet.
     * Returns the signature as a Uint8Array.
     */
    signMessage: async function(message) {
        if (!this.connected || !this.provider) {
            throw new Error('Wallet not connected');
        }

        var encoded = new TextEncoder().encode(message);
        var result = await this.provider.signMessage(encoded, 'utf8');
        // Phantom returns { signature }, Solflare returns the signature directly
        return result.signature || result;
    },

    /**
     * Sign and send a transaction (for future on-chain operations).
     */
    signAndSendTransaction: async function(transaction) {
        if (!this.connected || !this.provider) {
            throw new Error('Wallet not connected');
        }

        var result = await this.provider.signAndSendTransaction(transaction);
        return result;
    },

    // ========================================================================
    // Backend Registration
    // ========================================================================

    /**
     * Register the wallet with the AMOS backend.
     * Signs a verification message and POSTs the signature.
     */
    registerWallet: async function() {
        if (!this.connected || !this.publicKey) {
            throw new Error('Wallet not connected');
        }

        // Create a verification message with nonce (Unix seconds) to prevent replay
        var nonce = Math.floor(Date.now() / 1000).toString();
        var message = 'AMOS wallet verification: ' + this.publicKey + ' at ' + nonce;

        // Sign the verification message
        var signature = await this.signMessage(message);

        // Convert signature to byte array for JSON transport
        var sigBytes = Array.from(new Uint8Array(signature));

        // Register with backend
        var response = await fetch('/api/v1/wallet/connect', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            credentials: 'include',
            body: JSON.stringify({
                public_key: this.publicKey,
                signature: sigBytes,
                message: message,
                nonce: nonce,
            }),
        });

        if (!response.ok) {
            var errData = await response.json().catch(function() { return {}; });
            throw new Error(errData.error || 'Failed to register wallet (HTTP ' + response.status + ')');
        }

        return await response.json();
    },

    // ========================================================================
    // Balance
    // ========================================================================

    /**
     * Fetch token balance from the backend.
     */
    getTokenBalance: async function() {
        if (!this.connected || !this.publicKey) return null;

        try {
            var response = await fetch('/api/v1/wallet/balance?address=' + encodeURIComponent(this.publicKey), {
                credentials: 'include',
            });
            if (!response.ok) return null;

            var data = await response.json();
            return data;
        } catch (e) {
            console.warn('Failed to fetch wallet balance:', e);
            return null;
        }
    },

    /**
     * Refresh balance and update display.
     */
    refreshBalance: async function() {
        var data = await this.getTokenBalance();
        if (data) {
            this.balance = data;
        }
        this.updateUI();
    },

    // ========================================================================
    // Auto-Reconnect
    // ========================================================================

    /**
     * Attempt to silently reconnect on page load if the user previously connected.
     * Uses onlyIfTrusted (Phantom) to avoid showing the popup.
     */
    autoReconnect: async function() {
        var savedProvider = localStorage.getItem('amos-wallet-provider');
        if (!savedProvider) return;

        var wallets = this.getAvailableWallets();
        var wallet = wallets.find(function(w) { return w.name === savedProvider; });
        if (!wallet) return;

        try {
            // Use onlyIfTrusted to silently reconnect (no popup)
            var resp = await wallet.provider.connect({ onlyIfTrusted: true });
            var pubkey = resp.publicKey || wallet.provider.publicKey;
            if (!pubkey) return;

            this.provider = wallet.provider;
            this.walletName = wallet.name;
            this.publicKey = pubkey.toString();
            this.connected = true;

            this._setupListeners();

            // Silently refresh balance (no registration needed for reconnect)
            await this.refreshBalance();

            this.updateUI();
            console.log('Wallet auto-reconnected:', this.walletName, this.publicKey);
        } catch (e) {
            // Silent fail — user will need to manually connect
            console.log('Wallet auto-reconnect skipped:', e.message || e);
            localStorage.removeItem('amos-wallet-provider');
        }
    },

    // ========================================================================
    // UI Updates
    // ========================================================================

    /**
     * Update all wallet-related UI elements:
     * - Header status dot
     * - Settings section wallet display
     * - Elements with [data-requires-wallet]
     */
    updateUI: function() {
        // Header status dot
        var dot = document.getElementById('wallet-status-dot');
        if (dot) {
            if (this.connected) {
                dot.className = 'w-1.5 h-1.5 rounded-full bg-green-500';
            } else {
                dot.className = 'w-1.5 h-1.5 rounded-full bg-gray-500';
            }
        }

        // Header indicator tooltip
        var indicator = document.getElementById('wallet-header-indicator');
        if (indicator) {
            if (this.connected) {
                var shortAddr = this.publicKey.slice(0, 4) + '...' + this.publicKey.slice(-4);
                indicator.title = 'Wallet: ' + shortAddr;
            } else {
                indicator.title = 'Wallet not connected \u2014 click to set up';
            }
        }

        // Wallet settings section (if visible)
        this._updateSettingsSection();

        // Elements that require wallet connection
        var requiresWallet = document.querySelectorAll('[data-requires-wallet]');
        for (var i = 0; i < requiresWallet.length; i++) {
            if (this.connected) {
                requiresWallet[i].classList.remove('opacity-50', 'pointer-events-none');
                requiresWallet[i].removeAttribute('disabled');
            } else {
                requiresWallet[i].classList.add('opacity-50', 'pointer-events-none');
                requiresWallet[i].setAttribute('disabled', 'true');
            }
        }
    },

    /**
     * Update the wallet settings section content.
     */
    _updateSettingsSection: function() {
        var section = document.getElementById('wallet-settings-content');
        if (!section) return;

        if (this.connected) {
            var shortAddr = this.publicKey.slice(0, 4) + '...' + this.publicKey.slice(-4);
            var balanceText = '--';
            if (this.balance && this.balance.balance !== undefined) {
                balanceText = parseFloat(this.balance.balance).toLocaleString() + ' AMOS';
            }
            var tokenText = '';
            if (this.balance && this.balance.raw_balance !== undefined) {
                tokenText = '<div class="text-xs text-gray-400 mt-1">' +
                    parseFloat(this.balance.raw_balance).toLocaleString() + ' AMOS tokens</div>';
            }

            section.innerHTML =
                '<div class="flex items-center justify-between">' +
                    '<div class="flex items-center gap-3">' +
                        '<div class="w-8 h-8 rounded-full bg-green-500/20 flex items-center justify-center">' +
                            '<i data-lucide="wallet" class="w-4 h-4 text-green-500"></i>' +
                        '</div>' +
                        '<div>' +
                            '<div class="flex items-center gap-2">' +
                                '<span class="text-sm font-medium text-white">' + escapeHtml(shortAddr) + '</span>' +
                                '<button onclick="copyWalletAddress()" class="p-0.5 rounded hover:bg-gray-700 text-gray-400 hover:text-gray-200 transition-colors" title="Copy full address">' +
                                    '<i data-lucide="copy" class="w-3 h-3"></i>' +
                                '</button>' +
                            '</div>' +
                            '<div class="text-xs text-gray-400 capitalize">' + escapeHtml(this.walletName || 'Unknown') + '</div>' +
                        '</div>' +
                    '</div>' +
                    '<button onclick="AMOSWallet.disconnect()" class="px-3 py-1.5 rounded-lg text-xs font-medium bg-gray-700 hover:bg-gray-600 text-gray-300 hover:text-white transition-colors">' +
                        'Disconnect' +
                    '</button>' +
                '</div>' +
                '<div class="mt-3 p-3 rounded-lg bg-gray-800/50 border border-gray-700">' +
                    '<div class="text-xs text-gray-500 uppercase tracking-wider mb-1">Balance</div>' +
                    '<div class="text-sm font-medium text-white">' + escapeHtml(balanceText) + '</div>' +
                    tokenText +
                    '<button onclick="AMOSWallet.refreshBalance()" class="mt-2 text-xs text-amos-400 hover:text-amos-300 transition-colors">' +
                        'Refresh balance' +
                    '</button>' +
                '</div>';

            if (typeof lucide !== 'undefined') lucide.createIcons();
        } else {
            var availableWallets = this.getAvailableWallets();
            var html = '';

            if (availableWallets.length === 0) {
                html =
                    '<div class="text-center py-4">' +
                        '<div class="w-10 h-10 rounded-full bg-gray-800 flex items-center justify-center mx-auto mb-3">' +
                            '<i data-lucide="wallet" class="w-5 h-5 text-gray-500"></i>' +
                        '</div>' +
                        '<p class="text-sm text-gray-400 mb-2">No Solana wallet detected</p>' +
                        '<p class="text-xs text-gray-500 mb-3">Install a wallet extension to connect</p>' +
                        '<div class="flex justify-center gap-3">' +
                            '<a href="https://phantom.app/" target="_blank" rel="noopener" class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-gray-700 hover:bg-gray-600 text-gray-300 hover:text-white transition-colors">' +
                                '<img src="/static/img/phantom-icon.svg" alt="" class="w-4 h-4"> Get Phantom' +
                            '</a>' +
                            '<a href="https://solflare.com/" target="_blank" rel="noopener" class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium bg-gray-700 hover:bg-gray-600 text-gray-300 hover:text-white transition-colors">' +
                                '<img src="/static/img/solflare-icon.svg" alt="" class="w-4 h-4"> Get Solflare' +
                            '</a>' +
                        '</div>' +
                    '</div>';
            } else {
                html =
                    '<div class="text-center py-2 mb-3">' +
                        '<p class="text-sm text-gray-400">Connect a Solana wallet to access token features</p>' +
                    '</div>' +
                    '<div class="space-y-2">';

                for (var i = 0; i < availableWallets.length; i++) {
                    var w = availableWallets[i];
                    html +=
                        '<button onclick="AMOSWallet.connect(\'' + escapeHtml(w.name) + '\').catch(function(e){ alert(e.message); })"' +
                        ' class="w-full flex items-center gap-3 px-4 py-3 rounded-lg bg-gray-800 hover:bg-gray-700 border border-gray-700 hover:border-gray-600 transition-colors">' +
                            '<img src="' + escapeHtml(w.icon) + '" alt="" class="w-6 h-6">' +
                            '<span class="text-sm font-medium text-white">' + escapeHtml(w.displayName) + '</span>' +
                            '<i data-lucide="chevron-right" class="w-4 h-4 text-gray-500 ml-auto"></i>' +
                        '</button>';
                }

                html += '</div>';
            }

            section.innerHTML = html;
            if (typeof lucide !== 'undefined') lucide.createIcons();
        }
    },

    // ========================================================================
    // Internal Helpers
    // ========================================================================

    _setupListeners: function() {
        var self = this;

        if (this.provider) {
            var onDisconnect = function() {
                console.log('Wallet disconnected by provider');
                self.connected = false;
                self.publicKey = null;
                self.provider = null;
                self.walletName = null;
                self.balance = null;
                localStorage.removeItem('amos-wallet-provider');
                self.updateUI();
            };

            var onAccountChanged = function(publicKey) {
                if (publicKey) {
                    self.publicKey = publicKey.toString();
                    console.log('Wallet account changed:', self.publicKey);
                    self.refreshBalance();
                } else {
                    // Account changed to null means disconnected
                    onDisconnect();
                }
            };

            this.provider.on('disconnect', onDisconnect);
            this.provider.on('accountChanged', onAccountChanged);

            this._listeners = [
                { event: 'disconnect', handler: onDisconnect },
                { event: 'accountChanged', handler: onAccountChanged },
            ];
        }
    },

    _removeListeners: function() {
        if (this.provider && this._listeners.length > 0) {
            for (var i = 0; i < this._listeners.length; i++) {
                var l = this._listeners[i];
                try {
                    this.provider.off(l.event, l.handler);
                } catch (e) {
                    // Some providers don't support off()
                }
            }
        }
        this._listeners = [];
    },
};

// ============================================================================
// Global Wallet UI Functions (called from HTML onclick handlers)
// ============================================================================

/**
 * Show the wallet settings panel / modal.
 * If a settings view exists, navigate to it. Otherwise open the wallet modal.
 */
function showWalletSettings() {
    var modal = document.getElementById('wallet-modal');
    if (modal) {
        // Build the modal content dynamically
        _buildWalletModalContent();
        modal.style.display = 'flex';
    }
}

/**
 * Show the wallet selection modal.
 */
function showWalletModal() {
    var modal = document.getElementById('wallet-modal');
    if (modal) {
        _buildWalletModalContent();
        modal.style.display = 'flex';
    }
}

/**
 * Close the wallet modal.
 */
function closeWalletModal() {
    var modal = document.getElementById('wallet-modal');
    if (modal) {
        modal.style.display = 'none';
    }
}

/**
 * Copy the wallet address to clipboard.
 */
function copyWalletAddress() {
    if (!AMOSWallet.publicKey) return;

    navigator.clipboard.writeText(AMOSWallet.publicKey).then(function() {
        // Brief visual feedback
        var btn = document.querySelector('[onclick="copyWalletAddress()"]');
        if (btn) {
            btn.title = 'Copied!';
            setTimeout(function() { btn.title = 'Copy full address'; }, 1500);
        }
    }).catch(function(err) {
        console.warn('Failed to copy wallet address:', err);
    });
}

/**
 * Build the wallet modal content based on current state.
 */
function _buildWalletModalContent() {
    var container = document.getElementById('wallet-modal-body');
    if (!container) return;

    if (AMOSWallet.connected) {
        // Show connected state with balance and disconnect
        var shortAddr = AMOSWallet.publicKey.slice(0, 4) + '...' + AMOSWallet.publicKey.slice(-4);
        var fullAddr = AMOSWallet.publicKey;
        var balanceText = '--';
        if (AMOSWallet.balance && AMOSWallet.balance.balance !== undefined) {
            balanceText = parseFloat(AMOSWallet.balance.balance).toLocaleString() + ' AMOS';
        }
        var tokenHtml = '';
        if (AMOSWallet.balance && AMOSWallet.balance.raw_balance !== undefined) {
            tokenHtml = '<div class="text-sm text-gray-400 mt-1">' +
                parseFloat(AMOSWallet.balance.raw_balance).toLocaleString() + ' AMOS tokens</div>';
        }

        container.innerHTML =
            '<div class="text-center mb-4">' +
                '<div class="w-12 h-12 rounded-full bg-green-500/20 flex items-center justify-center mx-auto mb-3">' +
                    '<i data-lucide="check-circle" class="w-6 h-6 text-green-500"></i>' +
                '</div>' +
                '<h3 class="text-lg font-semibold text-white mb-1">Wallet Connected</h3>' +
                '<div class="flex items-center justify-center gap-2">' +
                    '<span class="text-sm text-gray-400 font-mono">' + escapeHtml(shortAddr) + '</span>' +
                    '<button onclick="copyWalletAddress()" class="p-1 rounded hover:bg-gray-700 text-gray-400 hover:text-white transition-colors" title="Copy full address">' +
                        '<i data-lucide="copy" class="w-3.5 h-3.5"></i>' +
                    '</button>' +
                '</div>' +
                '<div class="text-xs text-gray-500 capitalize mt-1">' + escapeHtml(AMOSWallet.walletName || '') + '</div>' +
            '</div>' +
            '<div class="p-4 rounded-lg bg-gray-800/50 border border-gray-700 mb-4">' +
                '<div class="text-xs text-gray-500 uppercase tracking-wider mb-1">Balance</div>' +
                '<div class="text-lg font-semibold text-white">' + escapeHtml(balanceText) + '</div>' +
                tokenHtml +
            '</div>' +
            '<div class="flex gap-3">' +
                '<button onclick="AMOSWallet.refreshBalance()" class="flex-1 px-4 py-2.5 rounded-lg text-sm font-medium bg-gray-700 hover:bg-gray-600 text-gray-300 hover:text-white transition-colors">' +
                    'Refresh Balance' +
                '</button>' +
                '<button onclick="AMOSWallet.disconnect().then(function(){ _buildWalletModalContent(); })" class="flex-1 px-4 py-2.5 rounded-lg text-sm font-medium bg-red-500/20 hover:bg-red-500/30 text-red-400 hover:text-red-300 border border-red-500/30 transition-colors">' +
                    'Disconnect' +
                '</button>' +
            '</div>';

        if (typeof lucide !== 'undefined') lucide.createIcons();
    } else {
        // Show wallet selection
        var wallets = AMOSWallet.getAvailableWallets();
        var html = '';

        html += '<div class="text-center mb-4">' +
            '<div class="w-12 h-12 rounded-full bg-amos-500/20 flex items-center justify-center mx-auto mb-3">' +
                '<i data-lucide="wallet" class="w-6 h-6 text-amos-400"></i>' +
            '</div>' +
            '<h3 class="text-lg font-semibold text-white mb-1">Connect Wallet</h3>' +
            '<p class="text-sm text-gray-400">Link a Solana wallet to access token features and rewards</p>' +
        '</div>';

        if (wallets.length === 0) {
            html += '<div class="p-4 rounded-lg bg-gray-800/50 border border-gray-700 text-center mb-4">' +
                '<p class="text-sm text-gray-400 mb-3">No Solana wallet extension detected</p>' +
                '<div class="flex justify-center gap-3">' +
                    '<a href="https://phantom.app/" target="_blank" rel="noopener" class="inline-flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium bg-purple-600 hover:bg-purple-700 text-white transition-colors">' +
                        '<img src="/static/img/phantom-icon.svg" alt="" class="w-5 h-5"> Install Phantom' +
                    '</a>' +
                    '<a href="https://solflare.com/" target="_blank" rel="noopener" class="inline-flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium bg-orange-500 hover:bg-orange-600 text-white transition-colors">' +
                        '<img src="/static/img/solflare-icon.svg" alt="" class="w-5 h-5"> Install Solflare' +
                    '</a>' +
                '</div>' +
            '</div>';
        } else {
            html += '<div class="space-y-2 mb-4">';
            for (var i = 0; i < wallets.length; i++) {
                var w = wallets[i];
                html += '<button onclick="AMOSWallet.connect(\'' + escapeHtml(w.name) + '\').then(function(){ _buildWalletModalContent(); }).catch(function(e){ alert(e.message); })"' +
                    ' class="w-full flex items-center gap-3 px-4 py-3.5 rounded-lg bg-gray-800 hover:bg-gray-700 border border-gray-700 hover:border-gray-600 transition-colors">' +
                        '<img src="' + escapeHtml(w.icon) + '" alt="" class="w-7 h-7">' +
                        '<div class="text-left flex-1">' +
                            '<div class="text-sm font-medium text-white">' + escapeHtml(w.displayName) + '</div>' +
                            '<div class="text-xs text-gray-500">Detected</div>' +
                        '</div>' +
                        '<i data-lucide="arrow-right" class="w-4 h-4 text-gray-500"></i>' +
                    '</button>';
            }
            html += '</div>';
        }

        container.innerHTML = html;
        if (typeof lucide !== 'undefined') lucide.createIcons();
    }
}

// ============================================================================
// Initialize on DOMContentLoaded
// ============================================================================

document.addEventListener('DOMContentLoaded', function() {
    // Attempt silent auto-reconnect
    AMOSWallet.autoReconnect();
});
