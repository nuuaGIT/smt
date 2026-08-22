use crate::storage::Language;

/// Format a number using the conventions of the selected UI language.
/// English uses `1,234.56`, German and Spanish use `1.234,56`, and French
/// uses a narrow non-breaking space for thousands: `1 234,56`.
pub fn format_number(language: Language, value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return value.to_string();
    }

    let negative = value.is_sign_negative();
    let absolute = value.abs();
    let raw = format!("{absolute:.decimals$}");
    let (integer, fraction) = raw.split_once('.').unwrap_or((&raw, ""));
    let separator = match language {
        Language::English => ',',
        Language::German | Language::Spanish => '.',
        Language::French => '\u{202f}',
    };
    let decimal = match language {
        Language::English => '.',
        Language::German | Language::French | Language::Spanish => ',',
    };

    let mut grouped = String::with_capacity(raw.len() + raw.len() / 3 + 2);
    if negative {
        grouped.push('-');
    }
    for (index, character) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index).is_multiple_of(3) {
            grouped.push(separator);
        }
        grouped.push(character);
    }
    if decimals > 0 {
        grouped.push(decimal);
        grouped.push_str(fraction);
    }
    grouped
}

/// Small, dependency-free UI dictionary. Savegame data and resource names stay
/// in their game-provided form; only application chrome and explanatory text
/// are translated here.
pub fn text(language: Language, key: &str) -> &'static str {
    let (english, german, french, spanish) = match key {
        "ready" => ("Ready", "Bereit", "Prêt", "Listo"),
        "loading" => ("Loading…", "Laden…", "Chargement…", "Cargando…"),
        "save_stored" => ("New savegame stored", "Neues Savegame gespeichert", "Nouvelle sauvegarde enregistrée", "Nueva partida guardada"),
        "no_changes" => ("No changes found", "Keine Änderungen gefunden", "Aucune modification trouvée", "No se encontraron cambios"),
        "analyzing_save" => ("Analyzing savegame …", "Savegame wird analysiert …", "Analyse de la sauvegarde …", "Analizando partida …"),
        "save_analyzed" => ("Savegame analyzed", "Savegame analysiert", "Sauvegarde analysée", "Partida analizada"),
        "analysis_failed" => ("Savegame could not be analyzed", "Savegame konnte nicht analysiert werden", "Impossible d'analyser la sauvegarde", "No se pudo analizar la partida"),
        "analysis_stopped" => ("Savegame analysis stopped", "Savegame-Analyse wurde beendet", "Analyse de la sauvegarde arrêtée", "Análisis de partida detenido"),
        "save_updated" => ("Savegame updated", "Savegame aktualisiert", "Sauvegarde mise à jour", "Partida actualizada"),
        "node_data_saved" => ("Node data saved", "Node-Daten gespeichert", "Données des nodes enregistrées", "Datos de nodos guardados"),
        "initialization_failed" => (
            "Initialization failed",
            "Initialisierung fehlgeschlagen",
            "Échec de l'initialisation",
            "Error de inicialización",
        ),
        "upload_save" => ("↑  Upload savegame", "↑  Savegame hochladen", "↑  Charger une sauvegarde", "↑  Cargar partida"),
        "refresh" => ("↻  Refresh", "↻  Refresh", "↻  Actualiser", "↻  Actualizar"),
        "remove_save" => ("×  Remove savegame", "×  Savegame entfernen", "×  Retirer la sauvegarde", "×  Quitar partida"),
        "settings" => ("Settings", "Einstellungen", "Paramètres", "Ajustes"),
        "settings_application" => ("Application", "Anwendung", "Application", "Aplicación"),
        "settings_map" => ("Map & nodes", "Karte & Nodes", "Carte et nodes", "Mapa y nodos"),
        "settings_display" => ("Display & performance", "Darstellung & Performance", "Affichage et performances", "Pantalla y rendimiento"),
        "settings_testing" => ("Testing stuff", "Testing stuff", "Fonctions de test", "Funciones de prueba"),
        "language" => ("Language", "Sprache", "Langue", "Idioma"),
        "node_names" => ("Show node names", "Node-Namen anzeigen", "Afficher les noms des nodes", "Mostrar nombres de nodos"),
        "debug_mode" => ("Debug mode", "Debug-Modus", "Mode débogage", "Modo de depuración"),
        "pause_background" => ("Do not render map when unfocused", "Map bei Alt-Tab nicht rendern", "Ne pas afficher la carte en arrière-plan", "No renderizar el mapa en segundo plano"),
        "auto_refresh" => ("Auto-refresh (minutes)", "Auto-Refresh (Minuten)", "Actualisation automatique (minutes)", "Actualización automática (minutos)"),
        "analysis_duration" => ("Analysis", "Analyse", "Analyse", "Análisis"),
        "last_updated" => ("Last update", "Letzte Aktualisierung", "Dernière mise à jour", "Última actualización"),
        "show_annotations" => ("Show drawings and shapes", "Gemaltes und Formen anzeigen", "Afficher les dessins et formes", "Mostrar dibujos y formas"),
        "more" => ("More", "Mehr", "Plus", "Más"),
        "delete_all_smt_notes" => ("Delete All SMT Notes", "Alle SMT-Notizen löschen", "Supprimer toutes les notes SMT", "Eliminar todas las notas de SMT"),
        "delete_all_smt_notes_title" => ("Delete All SMT Notes?", "Alle SMT-Notizen löschen?", "Supprimer toutes les notes SMT ?", "¿Eliminar todas las notas de SMT?"),
        "delete_all_smt_notes_warning" => ("This removes all SMT data for every savegame and unloads the current save. Original Satisfactory save files are not modified.", "Dadurch werden alle SMT-Daten für jedes Savegame gelöscht und das aktuelle Savegame entladen. Originale Satisfactory-Savegames werden nicht verändert.", "Toutes les données SMT de chaque sauvegarde seront supprimées et la sauvegarde actuelle sera déchargée. Les fichiers Satisfactory originaux ne sont pas modifiés.", "Esto elimina todos los datos de SMT de cada partida y descarga la partida actual. Los archivos originales de Satisfactory no se modifican."),
        "delete_all_smt_notes_confirm" => ("Delete everything", "Alles löschen", "Tout supprimer", "Eliminar todo"),
        "cancel" => ("Cancel", "Abbrechen", "Annuler", "Cancelar"),
        "rails_wip" => ("Show rails", "Rails anzeigen", "Afficher les rails", "Mostrar raíles"),
        "show_foundations" => ("Show foundations", "Foundations anzeigen", "Afficher les fondations", "Mostrar cimientos"),
        "show_belts" => ("Show belts", "Belts anzeigen", "Mostrar les convoyeurs", "Mostrar cintas"),
        "node_size" => ("Node size", "Node-Größe", "Taille des nodes", "Tamaño de los nodos"),
        "background_throttle" => ("Rendering is automatically throttled in the background.", "Im Hintergrund wird die Darstellung automatisch gedrosselt.", "Le rendu est automatiquement limité en arrière-plan.", "El renderizado se ralentiza automáticamente en segundo plano."),
        "map_panel" => ("Map panel", "Map-Panel", "Panneau de carte", "Panel del mapa"),
        "panel_open_hint" => ("Move the cursor to the right edge to open.", "Zum Öffnen den Cursor an den rechten Fensterrand bewegen.", "Placez le curseur sur le bord droit pour ouvrir.", "Mueve el cursor al borde derecho para abrirlo."),
        "close_panel" => ("×  Close panel", "×  Panel schließen", "×  Fermer le panneau", "×  Cerrar panel"),
        "resource_nodes" => ("Resource nodes", "Resource-Nodes", "Nodes de ressources", "Nodos de recursos"),
        "nodes" => ("nodes", "Nodes", "nodes", "nodos"),
        "nodes_loaded" => ("nodes loaded", "Nodes geladen", "nodes chargés", "nodos cargados"),
        "visible_nodes" => ("visible nodes", "sichtbare Nodes", "nodes visibles", "nodos visibles"),
        "remaining_short" => ("remaining", "übrig", "restantes", "restantes"),
        "zoom" => ("Zoom", "Zoom", "Zoom", "Zoom"),
        "cursor_world" => ("Cursor · World", "Cursor · Welt", "Curseur · Monde", "Cursor · Mundo"),
        "rails" => ("rails", "Schienen", "rails", "raíles"),
        "foundation_count" => ("foundations", "Foundations", "fondations", "cimientos"),
        "belt_count" => ("belts", "Belts", "convoyeurs", "cintas"),
        "belts" => ("belts", "Belts", "convoyeurs", "cintas"),
        "claimed_nodes" => ("claimed nodes", "geclaimte Nodes", "nodes revendiqués", "nodos reclamados"),
        "belts_laid" => ("Belts laid", "Belts verlegt", "Convoyeurs posés", "Cintas instaladas"),
        "rails_laid" => ("Rails laid", "Schienen verlegt", "Rails posés", "Raíles instalados"),
        "playtime" => ("Playtime", "Spielzeit", "Temps de jeu", "Tiempo de juego"),
        "save_file" => ("Save", "Savegame", "Sauvegarde", "Guardado"),
        "max_unclaimed" => ("Maximum capacity of unclaimed nodes", "Maximale Kapazität unclaimed Nodes", "Capacité maximale des nodes non revendiqués", "Capacidad máxima de nodos sin reclamar"),
        "unclaimed_hint" => ("Unclaimed nodes use the selected miner at the maximum 250% clock.", "Unclaimed werden mit dem gewählten Miner bei maximal 250% berechnet.", "Les nodes non revendiqués utilisent le mineur choisi à 250% maximum.", "Los nodos sin reclamar usan el minero seleccionado al 250% máximo."),
        "panel_resize_hint" => ("Drag the left edge to resize. Push it to the right edge to close.", "Panelbreite durch Ziehen der linken Kante ändern. Ganz nach rechts zusammenschieben schließt es.", "Tirez le bord gauche pour redimensionner. Repoussez-le vers le bord droit pour fermer.", "Arrastra el borde izquierdo para cambiar el tamaño. Llévalo al borde derecho para cerrarlo."),
        "app_title" => ("Satisfactory Map Tracker", "Satisfactory Map Tracker", "Satisfactory Map Tracker", "Satisfactory Map Tracker"),
        "draw_rectangle" => ("▭  Draw rectangle", "▭  Rechteck zeichnen", "▭  Dessiner un rectangle", "▭  Dibujar rectángulo"),
        "drawing_toolbar" => ("Draw", "Zeichnen", "Dessiner", "Dibujar"),
        "tool_rectangle" => ("Rectangle", "Rechteck", "Rectangle", "Rectángulo"),
        "cancel_rectangle" => ("×  Cancel drawing", "×  Zeichnen abbrechen", "×  Annuler le dessin", "×  Cancelar dibujo"),
        "draw_circle" => ("○  Draw circle", "○  Kreis zeichnen", "○  Dessiner un cercle", "○  Dibujar círculo"),
        "tool_circle" => ("Circle", "Kreis", "Cercle", "Círculo"),
        "cancel_circle" => ("×  Cancel circle", "×  Kreis abbrechen", "×  Annuler le cercle", "×  Cancelar círculo"),
        "draw_arrow" => ("➜  Draw arrow", "➜  Pfeil zeichnen", "➜  Dessiner une flèche", "➜  Dibujar flecha"),
        "tool_arrow" => ("Arrow", "Pfeil", "Flèche", "Flecha"),
        "cancel_arrow" => ("×  Cancel arrow", "×  Pfeil abbrechen", "×  Annuler la flèche", "×  Cancelar flecha"),
        "draw_ruler" => ("⌁  Ruler", "⌁  Lineal", "⌁  Règle", "⌁  Regla"),
        "tool_ruler" => ("Ruler", "Lineal", "Règle", "Regla"),
        "cancel_ruler" => ("×  Stop ruler", "×  Lineal stoppen", "×  Arrêter la règle", "×  Detener regla"),
        "draw_text" => ("T  Add text", "T  Text hinzufügen", "T  Ajouter du texte", "T  Añadir texto"),
        "tool_text" => ("Text", "Text", "Texte", "Texto"),
        "cancel_text" => ("×  Cancel text", "×  Text abbrechen", "×  Annuler le texte", "×  Cancelar texto"),
        "draw_pen" => ("✎  Draw", "✎  Stift", "✎  Dessiner", "✎  Dibujar"),
        "tool_pen" => ("Pen", "Stift", "Stylo", "Lápiz"),
        "cancel_pen" => ("×  Stop pen", "×  Stift stoppen", "×  Arrêter le stylo", "×  Detener lápiz"),
        "draw_eraser" => ("⌫  Eraser", "⌫  Radierer", "⌫  Gomme", "⌫  Borrador"),
        "tool_eraser" => ("Eraser", "Radierer", "Gomme", "Borrador"),
        "cancel_eraser" => ("×  Stop eraser", "×  Radierer stoppen", "×  Arrêter la gomme", "×  Detener borrador"),
        "text_label" => ("Text", "Text", "Texte", "Texto"),
        "save_text" => ("Save", "Speichern", "Enregistrer", "Guardar"),
        "cancel_text_edit" => ("Cancel", "Abbrechen", "Annuler", "Cancelar"),
        "rectangle" => ("Rectangle", "Rechteck", "Rectangle", "Rectángulo"),
        "circle" => ("Circle", "Kreis", "Cercle", "Círculo"),
        "arrow" => ("Arrow", "Pfeil", "Flèche", "Flecha"),
        "ruler" => ("Ruler", "Lineal", "Règle", "Regla"),
        "distance" => ("Distance", "Distanz", "Distance", "Distancia"),
        "delete_annotation" => ("Delete drawing", "Zeichnung löschen", "Supprimer le dessin", "Eliminar dibujo"),
        "move_annotation" => ("Move", "Verschieben", "Déplacer", "Mover"),
        "move_rectangle" => ("Move", "Verschieben", "Déplacer", "Mover"),
        "delete_rectangle" => ("Delete", "Löschen", "Supprimer", "Eliminar"),
        "error" => ("Error", "Fehler", "Erreur", "Error"),
        "not_initialized" => ("The application could not be initialized.", "Die Anwendung konnte nicht initialisiert werden.", "L'application n'a pas pu être initialisée.", "No se pudo inicializar la aplicación."),
        "no_save" => ("No Satisfactory savegame selected.", "Kein Satisfactory-Savegame ausgewählt.", "Aucune sauvegarde Satisfactory sélectionnée.", "No hay ninguna partida de Satisfactory seleccionada."),
        "upload_hint" => ("Please upload a savegame above.", "Bitte oben ein Savegame hochladen.", "Chargez une sauvegarde ci-dessus.", "Carga una partida arriba."),
        "map_paused" => ("Map rendering is paused while the window is in the background.", "Karten-Rendering pausiert, solange das Fenster im Hintergrund ist.", "Le rendu de la carte est en pause lorsque la fenêtre est en arrière-plan.", "El renderizado del mapa está pausado cuando la ventana está en segundo plano."),
        "change_in_settings" => ("(Can be changed in Settings)", "(In den Einstellungen änderbar)", "(Modifiable dans les paramètres)", "(Se puede cambiar en Ajustes)"),
        "save_status" => ("Savegame status", "Savegame-Status", "État de la sauvegarde", "Estado de la partida"),
        "no_saved_save" => ("No savegame stored yet.", "Noch kein Savegame gespeichert.", "Aucune sauvegarde enregistrée.", "Todavía no hay ninguna partida guardada."),
        "local_copy" => ("Local copy", "Lokale Kopie", "Copie locale", "Copia local"),
        "resource_remaining" => ("Resources used", "Verwendete Ressourcen", "Ressources utilisées", "Recursos utilizados"),
        "no_extractors" => ("No placed extractor data loaded yet.", "Noch keine platzierten Extractor-Daten geladen.", "Aucune donnée d'extracteur placé n'est encore chargée.", "Aún no se han cargado extractores colocados."),
        "available_per_minute" => ("available per minute", "pro Minute verfügbar", "disponibles par minute", "disponibles por minuto"),
        "not_fully_used" => ("not fully used", "nicht vollständig genutzt", "non utilisés complètement", "no utilizados por completo"),
        "resource" => ("Resource", "Ressource", "Ressource", "Recurso"),
        "purity" => ("Purity", "Reinheit", "Pureté", "Pureza"),
        "all_resources" => ("All resources", "Alle Ressourcen", "Toutes les ressources", "Todos los recursos"),
        "no_resources_selected" => ("No resources selected", "Keine Ressourcen ausgewählt", "Aucune ressource sélectionnée", "No hay recursos seleccionados"),
        "resources_selected" => ("selected", "ausgewählt", "sélectionnées", "seleccionados"),
        "activate_all" => ("Activate all", "Alle aktivieren", "Tout activer", "Activar todos"),
        "deactivate_all" => ("Deactivate all", "Alle deaktivieren", "Tout désactiver", "Desactivar todos"),
        "all_purities" => ("All purities", "Alle Reinheiten", "Toutes les puretés", "Todas las purezas"),
        "claimed_only" => ("Claimed nodes only", "Nur geclaimte Nodes", "Nodes revendiqués uniquement", "Solo nodos reclamados"),
        "partial_only" => ("Partially used nodes only", "Nur nicht vollständig genutzte Nodes", "Nodes partiellement utilisés uniquement", "Solo nodos parcialmente usados"),
        "planned_only" => ("Planned nodes only", "Nur geplante Nodes", "Nodes planifiés uniquement", "Solo nodos planificados"),
        "grid" => ("Show coordinate grid", "Koordinatenraster anzeigen", "Afficher la grille de coordonnées", "Mostrar cuadrícula de coordenadas"),
        "detailed_png_map" => ("Use detailed PNG map", "Detaillierte PNG-Karte verwenden", "Utiliser la carte PNG détaillée", "Usar mapa PNG detallado"),
        "world_data_hint" => ("The map and positions come from local world data.", "Karte und Positionen stammen aus den lokalen Welt-Daten.", "La carte et les positions proviennent des données locales du monde.", "El mapa y las posiciones proceden de los datos locales del mundo."),
        "reset_view" => ("Reset view", "Ansicht zurücksetzen", "Réinitialiser la vue", "Restablecer vista"),
        "close" => ("Close", "Schließen", "Fermer", "Cerrar"),
        "usage" => ("Usage", "Ressourcenverbrauch", "Utilisation", "Uso"),
        "note" => ("Note", "Notiz", "Note", "Nota"),
        "yield" => ("Yield", "Ertrag", "Rendement", "Producción"),
        "theoretical_yield" => ("Theoretical miner yield", "Theoretischer Miner-Ertrag", "Rendement théorique du mineur", "Producción teórica del minero"),
        "maximum_settable" => ("Maximum settable", "Maximal einstellbar", "Maximum réglable", "Máximo configurable"),
        "max_value" => ("Max", "Max", "Max", "Máx."),
        "max_possible" => ("Max possible", "Maximal möglich", "Maximum possible", "Máximo posible"),
        "powershards_clock" => ("Powershards", "Powershards", "Powershards", "Powershards"),
        "used_per_minute" => ("used per minute", "pro Minute verwendet", "utilisés par minute", "usados por minuto"),
        "free_per_minute" => ("Free", "Frei", "Libre", "Libre"),
        "occupancy" => ("Occupancy", "Belegung", "Occupation", "Ocupación"),
        "node_id" => ("Node ID", "Node-ID", "ID du node", "ID del nodo"),
        "world" => ("World", "Welt", "Monde", "Mundo"),
        "extractor_unknown" => ("Extractor not found in savegame", "Extractor keiner in der Save-Datei erkannt", "Extracteur introuvable dans la sauvegarde", "Extractor no encontrado en la partida"),
        "method" => ("Method", "Methode", "Méthode", "Método"),
        "rate_per_satellite" => ("Rate applies per resource-well satellite.", "Rate gilt pro Resource-Well-Satellit.", "Le débit s'applique par satellite du puits.", "La tasa se aplica por satélite del pozo."),
        "well_core" => ("Central resource-well core", "Zentraler Resource-Well-Kern", "Cœur central du puits de ressources", "Núcleo central del pozo de recursos"),
        "well_yield_hint" => ("Yield comes from the connected satellite nodes.", "Die Ausbeute kommt von den verbundenen Satelliten-Nodes.", "Le rendement vient des nodes satellites connectés.", "La producción procede de los nodos satélite conectados."),
        "manual_deposit" => ("Manual deposit – no miner extractor", "Manuelles Deposit – kein Miner-Extractor", "Gisement manuel – aucun extracteur", "Depósito manual – sin extractor minero"),
        "your_entry" => ("Your entry", "Dein Eintrag", "Votre saisie", "Tu entrada"),
        "satellite_usage" => ("Usage", "Nutzung", "Utilisation", "Uso"),
        "oil_extractor" => ("Oil Extractor", "Oil Extractor", "Extracteur de pétrole", "Extractor de petróleo"),
        "well_extractor" => ("Resource Well Extractor", "Resource Well Extractor", "Extracteur de puits de ressources", "Extractor de pozo de recursos"),
        "miner" => ("Miner", "Miner", "Mineur", "Minero"),
        "theoretical" => ("Theoretical", "Theoretischer", "Théorique", "Teórico"),
        "output" => ("Output", "Output", "Sortie", "Producción"),
        "satellites_total" => ("Total satellites", "Satelliten gesamt", "Total des satellites", "Satélites totales"),
        "satellites_in_well" => ("satellites in well", "Satelliten im Well", "satellites dans le puits", "satélites en el pozo"),
        "claimed" => ("●  Claimed", "●  Geclaimt", "●  Revendiqué", "●  Reclamado"),
        "unclaimed" => ("○  Unclaimed", "○  Nicht geclaimt", "○  Non revendiqué", "○  Sin reclamar"),
        _ => ("", "", "", ""),
    };

    match language {
        Language::English => english,
        Language::German => german,
        Language::French => french,
        Language::Spanish => spanish,
    }
}

#[cfg(test)]
mod tests {
    use super::format_number;
    use crate::storage::Language;

    #[test]
    fn formats_numbers_using_each_locale() {
        assert_eq!(
            format_number(Language::English, 1_234_567.89, 2),
            "1,234,567.89"
        );
        assert_eq!(
            format_number(Language::German, 1_234_567.89, 2),
            "1.234.567,89"
        );
        assert_eq!(
            format_number(Language::Spanish, 1_234_567.89, 2),
            "1.234.567,89"
        );
        assert_eq!(
            format_number(Language::French, 1_234_567.89, 2),
            "1\u{202f}234\u{202f}567,89"
        );
    }
}
