'use client';

import React, { useState, useEffect } from 'react';
import { Token, TokenSelect } from '@ant-design/web3';
import { ETH, USDC, USDT } from '@ant-design/web3-assets/tokens';
import {
    WBTCCircleColorful,
    SolanaCircleColorful,
    PolygonCircleColorful,
    ChainlinkCircleColorful,
    AvaxCircleColorful,
} from '@ant-design/web3-icons';

const WBTC: Token = {
    symbol: 'WBTC',
    name: 'Wrapped Bitcoin',
    decimal: 8,
    icon: <WBTCCircleColorful />,
    availableChains: [],
};

const SOL: Token = {
    symbol: 'SOL',
    name: 'Solana',
    decimal: 9,
    icon: <SolanaCircleColorful />,
    availableChains: [],
};

const MATIC: Token = {
    symbol: 'MATIC',
    name: 'Polygon',
    decimal: 18,
    icon: <PolygonCircleColorful />,
    availableChains: [],
};

const LINK: Token = {
    symbol: 'LINK',
    name: 'Chainlink',
    decimal: 18,
    icon: <ChainlinkCircleColorful />,
    availableChains: [],
};

const AVAX: Token = {
    symbol: 'AVAX',
    name: 'Avalanche',
    decimal: 18,
    icon: <AvaxCircleColorful />,
    availableChains: [],
};

const tokens: Token[] = [
    {
        ...ETH,
        symbol: 'WETH',
        name: 'Wrapped Ethereum'
    },
    USDC,
    USDT,
    WBTC,
    SOL,
];

interface TokenPairPickerProps {
    onPairChange?: (pairString: string) => void;
}

export default function TokenPairPicker({ onPairChange }: TokenPairPickerProps) {
    const [tokenA, setTokenA] = React.useState<Token>(tokens[0]);
    const [tokenB, setTokenB] = React.useState<Token>(tokens[1]);

    // prevent selecting same token for both slots
    const onChangeTokenA = (token: Token) => {
        if (token.symbol === tokenB.symbol) {
            setTokenB(tokenA);
        }
        setTokenA(token);
    };

    const onChangeTokenB = (token: Token) => {
        if (token.symbol === tokenA.symbol) {
            setTokenA(tokenB);
        }
        setTokenB(token);
    };

    useEffect(() => {
        const pairString = `${tokenA.symbol}-${tokenB.symbol}`;
        onPairChange?.(pairString);
    }, [tokenA, tokenB, onPairChange]);

    return (
        <div style={{ maxWidth: 400, margin: 'auto' }}>
            <div style={{ marginBottom: 16 }}>
                <h3>Select Token A</h3>
                <TokenSelect
                    options={tokens}
                    value={tokenA}
                    onChange={onChangeTokenA}
                    style={{ width: '100%' }}
                />
            </div>
            <div style={{ marginBottom: 16 }}>
                <h3>Select Token B</h3>
                <TokenSelect
                    options={tokens}
                    value={tokenB}
                    onChange={onChangeTokenB}
                    style={{ width: '100%' }}
                />
            </div>
        </div>
    );
}
