use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use redash_types::DevicePairingPayload;

/// Generates a valid standard SVG string representation of the QR Code.
pub fn generate_pairing_qr_svg(payload: &DevicePairingPayload) -> Result<String, String> {
    let uri = payload.to_uri();
    let code = QrCode::with_error_correction_level(uri.as_bytes(), EcLevel::M)
        .map_err(|e| format!("Failed to generate QR code: {}", e))?;

    let image = code
        .render::<svg::Color>()
        .min_dimensions(200, 200)
        .dark_color(svg::Color("#00ffcc")) // Neon cyan theme accent
        .light_color(svg::Color("#161822")) // Dark Tech background
        .build();

    Ok(image)
}

/// Generates an ASCII string representation of the QR Code suitable for terminals.
pub fn generate_pairing_qr_ascii(payload: &DevicePairingPayload) -> Result<String, String> {
    let uri = payload.to_uri();
    let code = QrCode::with_error_correction_level(uri.as_bytes(), EcLevel::L)
        .map_err(|e| format!("Failed to generate QR code: {}", e))?;

    let string = code
        .render::<char>()
        .quiet_zone(false)
        .module_dimensions(2, 1)
        .build();

    Ok(string)
}

/// Generates a 2D boolean matrix of modules (true = dark, false = light) for GPUI / Canvas rendering.
pub fn generate_pairing_qr_matrix(
    payload: &DevicePairingPayload,
) -> Result<Vec<Vec<bool>>, String> {
    let uri = payload.to_uri();
    let code = QrCode::with_error_correction_level(uri.as_bytes(), EcLevel::M)
        .map_err(|e| format!("Failed to generate QR code: {}", e))?;

    let width = code.width();
    let colors = code.to_colors();

    let mut matrix = Vec::with_capacity(width);
    for y in 0..width {
        let mut row = Vec::with_capacity(width);
        for x in 0..width {
            let color = colors[y * width + x];
            row.push(color == qrcode::Color::Dark);
        }
        matrix.push(row);
    }

    Ok(matrix)
}

/// Parses and verifies a pairing URI from a scanned QR code or deep link.
pub fn parse_pairing_uri(uri: &str) -> Result<DevicePairingPayload, String> {
    DevicePairingPayload::from_uri(uri)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pairing_qr_svg_generation() {
        let payload = DevicePairingPayload {
            hub_url: "ws://192.168.1.10:8080".to_string(),
            client_public_key: "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
                .to_string(),
            device_name: "MacBook Pro".to_string(),
            auth_token: Some("secret123".to_string()),
            node_id: None,
            created_at: 1700000000,
        };

        let svg = generate_pairing_qr_svg(&payload).unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));

        let ascii = generate_pairing_qr_ascii(&payload).unwrap();
        assert!(!ascii.is_empty());

        let matrix = generate_pairing_qr_matrix(&payload).unwrap();
        assert!(!matrix.is_empty());
        assert_eq!(matrix.len(), matrix[0].len());

        let uri = payload.to_uri();
        let parsed = parse_pairing_uri(&uri).unwrap();
        assert_eq!(parsed, payload);
    }
}
