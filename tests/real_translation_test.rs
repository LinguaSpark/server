use linguaspark_server::{perform_translation, AppState, AppError};
use linguaspark::Translator;
use isolang::Language;
use std::sync::Arc;
use std::path::PathBuf;
use std::fs;

// Helper function to create app state with real models
async fn create_real_app_state() -> Result<Arc<AppState>, Box<dyn std::error::Error>> {
    let translator = Translator::new(1)?;
    
    // Use the actual models directory from the project root
    let models_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("models");
    
    println!("Looking for models in: {}", models_dir.display());
    
    let mut models = Vec::new();
    
    // Load only the models we need for testing
    let needed_models = vec!["enzh", "zhen"];
    
    for model_name in needed_models {
        let model_path = models_dir.join(model_name);
        if model_path.exists() {
            println!("Loading model: {}", model_name);
            match translator.load_model(model_name, model_path) {
                Ok(_) => {
                    let from_lang = Language::from_639_1(&model_name[0..2]).unwrap();
                    let to_lang = Language::from_639_1(&model_name[2..4]).unwrap();
                    models.push((from_lang, to_lang));
                    println!("Successfully loaded model: {}", model_name);
                }
                Err(e) => {
                    println!("Failed to load model {}: {:?}", model_name, e);
                }
            }
        } else {
            println!("Model directory not found: {}", model_path.display());
        }
    }
    
    println!("Loaded {} models total", models.len());
    
    Ok(Arc::new(AppState { translator, models }))
}

#[tokio::test]
async fn test_webollama_with_real_models() {
    println!("=== Testing webollama with real models ===");
    
    match create_real_app_state().await {
        Ok(app_state) => {
            println!("Created app state with {} models", app_state.models.len());
            
            // Test the exact scenario that causes the null pointer error
            let test_text = "webollama";
            let to_lang = "zh";
            
            println!("Testing translation: '{}' -> '{}'", test_text, to_lang);
            
            match perform_translation(&app_state, test_text, None, to_lang).await {
                Ok((translated, from, to)) => {
                    println!("Translation successful:");
                    println!("  Original: {}", test_text);
                    println!("  Translated: {}", translated);
                    println!("  From: {}", from);
                    println!("  To: {}", to);
                }
                Err(e) => {
                    println!("Translation failed: {:?}", e);
                    match e {
                        AppError::TranslatorError(ref te) => {
                            println!("Translator error details: {:?}", te);
                            
                            // Check if this is the null pointer error we're looking for
                            let error_str = format!("{:?}", te);
                            if error_str.contains("null pointer") {
                                println!("Found the null pointer error!");
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        Err(e) => {
            println!("Failed to create app state: {:?}", e);
        }
    }
}

#[tokio::test]
async fn test_direct_translator_with_webollama() {
    println!("=== Testing direct translator with webollama ===");
    
    let translator = match Translator::new(1) {
        Ok(t) => t,
        Err(e) => {
            println!("Failed to create translator: {:?}", e);
            return;
        }
    };
    
    // Try to load enzh model
    let models_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("models");
    
    let enzh_path = models_dir.join("enzh");
    if enzh_path.exists() {
        println!("Loading enzh model from: {}", enzh_path.display());
        
        match translator.load_model("enzh", enzh_path) {
            Ok(_) => {
                println!("Successfully loaded enzh model");
                
                // Check if translation is supported
                match translator.is_supported("en", "zh") {
                    Ok(true) => {
                        println!("en -> zh translation is supported");
                        
                        // Try translating webollama
                        match translator.translate("en", "zh", "webollama") {
                            Ok(result) => {
                                println!("Translation successful: 'webollama' -> '{}'", result);
                            }
                            Err(e) => {
                                println!("Translation failed: {:?}", e);
                                
                                // Check if this is the null pointer error
                                let error_str = format!("{:?}", e);
                                if error_str.contains("null pointer") {
                                    println!("Found the null pointer error in direct translation!");
                                }
                            }
                        }
                    }
                    Ok(false) => {
                        println!("en -> zh translation is not supported");
                    }
                    Err(e) => {
                        println!("Error checking support: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("Failed to load enzh model: {:?}", e);
            }
        }
    } else {
        println!("enzh model directory not found: {}", enzh_path.display());
    }
}

#[tokio::test]
async fn test_various_words_with_real_models() {
    println!("=== Testing various words with real models ===");
    
    match create_real_app_state().await {
        Ok(app_state) => {
            let test_words = vec![
                "webollama",
                "hello",
                "world", 
                "test",
                "ollama",
                "web",
                "ai",
                "translation",
            ];
            
            for word in test_words {
                println!("\nTesting word: '{}'", word);
                
                match perform_translation(&app_state, word, None, "zh").await {
                    Ok((translated, from, to)) => {
                        println!("  Success: '{}' -> '{}' ({} -> {})", word, translated, from, to);
                    }
                    Err(e) => {
                        println!("  Error: {:?}", e);
                        if let AppError::TranslatorError(ref te) = e {
                            let error_str = format!("{:?}", te);
                            if error_str.contains("null pointer") {
                                println!("  ** NULL POINTER ERROR DETECTED for word: '{}' **", word);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            println!("Failed to create app state: {:?}", e);
        }
    }
} 